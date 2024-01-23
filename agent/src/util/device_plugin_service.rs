use super::constants::{
    HEALTHY, KUBELET_UPDATE_CHANNEL_CAPACITY, LIST_AND_WATCH_SLEEP_SECS, UNHEALTHY,
};
use super::v1beta1;
use super::v1beta1::{
    device_plugin_server::DevicePlugin, AllocateRequest, AllocateResponse, DevicePluginOptions,
    DeviceSpec, Empty, ListAndWatchResponse, Mount, PreStartContainerRequest,
    PreStartContainerResponse,
};
use akri_discovery_utils::discovery::v0::Device;
use akri_shared::{
    akri::{
        configuration::ConfigurationSpec,
        instance::device_usage::{DeviceUsageKind, NodeUsage},
        instance::InstanceSpec,
        retry::{random_delay, MAX_INSTANCE_UPDATE_TRIES},
        AKRI_SLOT_ANNOTATION_NAME_PREFIX,
    },
    k8s,
    k8s::KubeInterface,
};
use log::{error, info, trace};
#[cfg(test)]
use mock_instant::Instant;
#[cfg(not(test))]
use std::time::Instant;
use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
    sync::Arc,
    time::Duration,
};
use tokio::{
    sync::{broadcast, mpsc, RwLock},
    time::timeout,
};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Code, Request, Response, Status};

/// Message sent in channel to `list_and_watch`.
/// Dictates what action `list_and_watch` should take upon being awoken.
#[derive(PartialEq, Clone, Debug)]
pub enum ListAndWatchMessageKind {
    /// Prematurely continue looping
    Continue,
    /// Stop looping
    End,
}

/// Describes whether an instance was discovered or the time at which it was no longer discovered.
#[derive(PartialEq, Debug, Clone)]
pub enum InstanceConnectivityStatus {
    /// Was discovered
    Online,
    /// Could not be discovered. Instant contains time at which it was no longer discovered.
    Offline(Instant),
}

/// Contains an Instance's state
#[derive(Clone, Debug)]
pub struct InstanceInfo {
    /// Sender to tell `list_and_watch` to either prematurely continue looping or end
    pub list_and_watch_message_sender: broadcast::Sender<ListAndWatchMessageKind>,
    /// Instance's `InstanceConnectivityStatus`
    pub connectivity_status: InstanceConnectivityStatus,
    /// Instance's hash id
    pub instance_id: String,
    /// Device that the instance represents.
    /// Contains information about environment variables and volumes that should be mounted
    /// into requesting Pods.
    pub device: Device,
}

#[derive(Clone, Debug, Default)]
pub struct DevicePluginContext {
    /// Sender to tell Configuration device plugin `list_and_watch` to either prematurely continue looping or end
    pub usage_update_message_sender: Option<broadcast::Sender<ListAndWatchMessageKind>>,
    /// Map of all Instances from the same Configuration
    pub instances: HashMap<String, InstanceInfo>,
}

#[derive(Clone)]
pub enum DevicePluginBehavior {
    Configuration(ConfigurationDevicePlugin),
    Instance(InstanceDevicePlugin),
}

#[derive(PartialEq, Clone, Debug)]
pub enum DeviceUsageStatus {
    /// Free
    Free,
    /// Reserved by Configuration Device Plugin on current node
    ReservedByConfiguration(String),
    /// Reserved by Instance Device Plugin on current node
    ReservedByInstance,
    /// Reserved by other nodes
    ReservedByOtherNode,
    /// Unknown, insufficient information to determine the status,
    /// mostly due to the device usage slot is not found from the instance map
    Unknown,
}

/// Kubernetes Device-Plugin for an Instance.
///
/// `DevicePluginService` implements Kubernetes Device-Plugin v1beta1 API specification
/// defined in a public proto file (imported here at agent/proto/pluginapi.proto).
/// The code generated from pluginapi.proto can be found in `agent/src/util/v1beta1.rs`.
/// Each `DevicePluginService` has an associated Instance and Configuration.
/// Serves a unix domain socket, sending and receiving messages to/from kubelet.
/// Kubelet is its client, calling each of its methods.
#[derive(Clone)]
pub struct DevicePluginService {
    /// Instance CRD name
    pub instance_name: String,
    /// Instance's Configuration
    pub config: ConfigurationSpec,
    /// Name of Instance's Configuration CRD
    pub config_name: String,
    /// UID of Instance's Configuration CRD
    pub config_uid: String,
    /// Namespace of Instance's Configuration CRD
    pub config_namespace: String,
    /// Hostname of node this Device Plugin is running on
    pub node_name: String,
    /// Map of all Instances that have the same Configuration CRD as this one
    pub device_plugin_context: Arc<RwLock<DevicePluginContext>>,
    /// Receiver for list_and_watch continue or end messages
    /// Note: since the tonic grpc generated list_and_watch definition takes in &self,
    /// using broadcast sender instead of mpsc receiver
    /// Can clone broadcast sender and subscribe receiver to use in spawned thread in list_and_watch
    pub list_and_watch_message_sender: broadcast::Sender<ListAndWatchMessageKind>,
    /// Upon send, terminates function that acts as the shutdown signal for this service
    pub server_ender_sender: mpsc::Sender<()>,
    /// Enum object that defines the behavior of the device plugin
    pub device_plugin_behavior: DevicePluginBehavior,
}

#[tonic::async_trait]
impl DevicePlugin for DevicePluginService {
    /// Returns options to be communicated with kubelet Device Manager
    async fn get_device_plugin_options(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<DevicePluginOptions>, Status> {
        trace!("get_device_plugin_options - kubelet called get_device_plugin_options");
        let resp = DevicePluginOptions {
            pre_start_required: false,
        };
        Ok(Response::new(resp))
    }

    type ListAndWatchStream = ReceiverStream<Result<ListAndWatchResponse, Status>>;

    /// Called by Kubelet right after the DevicePluginService registers with Kubelet.
    /// Returns a stream of List of "virtual" Devices over a channel.
    /// Since Kubernetes designed Device-Plugin so that multiple consumers can use a Device,
    /// "virtual" Devices are reservation slots for using the Device or Instance in akri terms.
    /// The number of "virtual" Devices (length of `ListAndWatchResponse`) is determined by Instance.capacity.
    /// Whenever Instance state changes or an Instance disapears, `list_and_watch` returns the new list.
    /// Runs until receives message to end due to Instance disappearing or Configuration being deleted.
    async fn list_and_watch(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<Self::ListAndWatchStream>, Status> {
        info!(
            "list_and_watch - kubelet called list_and_watch for instance {}",
            self.instance_name
        );
        let kube_interface = Arc::new(k8s::KubeImpl::new().await.unwrap());
        self.internal_list_and_watch(kube_interface).await
    }

    /// Kubelet calls allocate during pod creation.
    /// This means kubelet is trying to reserve a usage slot (virtual Device) of the Instance for this node.
    /// Returns error if cannot reserve that slot.
    async fn allocate(
        &self,
        requests: Request<AllocateRequest>,
    ) -> Result<Response<AllocateResponse>, Status> {
        info!(
            "allocate - kubelet called allocate for Instance {}",
            self.instance_name
        );
        let kube_interface = Arc::new(k8s::KubeImpl::new().await.unwrap());
        self.internal_allocate(requests, kube_interface).await
    }

    /// Should never be called, as indicated by DevicePluginService during registration.
    async fn pre_start_container(
        &self,
        _request: Request<PreStartContainerRequest>,
    ) -> Result<Response<PreStartContainerResponse>, Status> {
        error!(
            "pre_start_container - kubelet called pre_start_container for Instance {}",
            self.instance_name
        );
        Ok(Response::new(v1beta1::PreStartContainerResponse {}))
    }
}

impl DevicePluginService {
    async fn internal_list_and_watch<'a>(
        &'a self,
        kube_interface: Arc<impl KubeInterface + 'a + 'static>,
    ) -> Result<Response<<DevicePluginService as DevicePlugin>::ListAndWatchStream>, Status> {
        let dps = Arc::new(self.clone());
        // Create a channel that list_and_watch can periodically send updates to kubelet on
        let (kubelet_update_sender, kubelet_update_receiver) =
            mpsc::channel(KUBELET_UPDATE_CHANNEL_CAPACITY);
        // Spawn thread so can send kubelet the receiving end of the channel to listen on
        tokio::spawn(async move {
            match &dps.device_plugin_behavior {
                DevicePluginBehavior::Configuration(dp) => {
                    dp.list_and_watch(
                        dps.clone(),
                        kube_interface,
                        kubelet_update_sender,
                        LIST_AND_WATCH_SLEEP_SECS,
                    )
                    .await
                }
                DevicePluginBehavior::Instance(dp) => {
                    dp.list_and_watch(
                        dps.clone(),
                        kube_interface,
                        kubelet_update_sender,
                        LIST_AND_WATCH_SLEEP_SECS,
                    )
                    .await
                }
            }
        });
        Ok(Response::new(ReceiverStream::new(kubelet_update_receiver)))
    }

    /// Called when kubelet is trying to reserve for this node a usage slot (or virtual device) of the Instance.
    /// Tries to update Instance CRD to reserve the requested slot. If cannot reserve that slot, forces `list_and_watch` to continue
    /// (sending kubelet the latest list of slots) and returns error, so kubelet will not schedule the pod to this node.
    async fn internal_allocate(
        &self,
        requests: Request<AllocateRequest>,
        kube_interface: Arc<impl KubeInterface>,
    ) -> Result<Response<AllocateResponse>, Status> {
        let dps = Arc::new(self.clone());
        match &dps.device_plugin_behavior {
            DevicePluginBehavior::Configuration(dp) => {
                dp.allocate(dps.clone(), requests, kube_interface).await
            }
            DevicePluginBehavior::Instance(dp) => {
                dp.allocate(dps.clone(), requests, kube_interface).await
            }
        }
    }
}

#[derive(Clone)]
pub struct InstanceDevicePlugin {
    /// Instance hash id
    pub instance_id: String,
    /// Instance is \[not\]shared
    pub shared: bool,
    /// Device that the instance represents.
    /// Contains information about environment variables and volumes that should be mounted
    /// into requesting Pods.
    pub device: Device,
}

impl InstanceDevicePlugin {
    async fn list_and_watch(
        &self,
        dps: Arc<DevicePluginService>,
        kube_interface: Arc<impl KubeInterface>,
        kubelet_update_sender: mpsc::Sender<Result<ListAndWatchResponse, Status>>,
        polling_interval_secs: u64,
    ) {
        let mut list_and_watch_message_receiver = dps.list_and_watch_message_sender.subscribe();
        let mut keep_looping = true;
        // Try to create an Instance CRD for this plugin and add it to the global InstanceMap else shutdown
        if let Err(e) = try_create_instance(dps.clone(), self, kube_interface.clone()).await {
            error!(
                "InstanceDevicePlugin::list_and_watch - ending service because could not create instance {} with error {}",
                dps.instance_name,
                e
            );
            dps.server_ender_sender.clone().send(()).await.unwrap();
            keep_looping = false;
        }

        let mut prev_virtual_devices: Vec<v1beta1::Device> = Vec::new();
        while keep_looping {
            trace!(
                "InstanceDevicePlugin::list_and_watch - loop iteration for Instance {}",
                dps.instance_name
            );

            let device_usage_states = get_instance_device_usage_states(
                &dps.node_name,
                &dps.instance_name,
                &dps.config_namespace,
                &dps.config.capacity,
                kube_interface.clone(),
            )
            .await;

            // Generate virtual devices, for Instance Device Plugin virtual devices,
            // the health state is healthy if the slot is free or
            // is reserved by Instance Device Plugin itself previously
            let virtual_devices = device_usage_states
                .into_iter()
                .map(|(slot, state)| v1beta1::Device {
                    id: slot,
                    health: match state {
                        DeviceUsageStatus::Free | DeviceUsageStatus::ReservedByInstance => {
                            HEALTHY.to_string()
                        }
                        _ => UNHEALTHY.to_string(),
                    },
                })
                .collect::<Vec<v1beta1::Device>>();
            // Only send the virtual devices if the list has changed
            if !(prev_virtual_devices
                .iter()
                .all(|item| virtual_devices.contains(item))
                && virtual_devices.len() == prev_virtual_devices.len())
            {
                prev_virtual_devices = virtual_devices.clone();
                let resp = v1beta1::ListAndWatchResponse {
                    devices: virtual_devices,
                };
                info!(
                    "InstanceDevicePlugin::list_and_watch - for device plugin {}, response = {:?}",
                    dps.instance_name, resp
                );
                // Send virtual devices list back to kubelet
                if let Err(e) = kubelet_update_sender.send(Ok(resp)).await {
                    trace!(
                        "InstanceDevicePlugin::list_and_watch - for Instance {} kubelet no longer receiving with error {}",
                        dps.instance_name,
                        e
                    );
                    // This means kubelet is down/has been restarted. Remove instance from instance map so
                    // do_periodic_discovery will create a new device plugin service for this instance.
                    dps.device_plugin_context
                        .write()
                        .await
                        .instances
                        .remove(&dps.instance_name);
                    dps.server_ender_sender.clone().send(()).await.unwrap();
                    keep_looping = false;
                } else {
                    // Notify device usage had been changed
                    if let Some(sender) = &dps
                        .device_plugin_context
                        .read()
                        .await
                        .usage_update_message_sender
                    {
                        if let Err(e) = sender.send(ListAndWatchMessageKind::Continue) {
                            error!("InstanceDevicePlugin::list_and_watch - fails to notify device usage, error {}", e);
                        }
                    }
                }
            }

            // Sleep for polling_interval_secs unless receive message to shutdown the server
            // or continue (and send another list of devices)
            match timeout(
                Duration::from_secs(polling_interval_secs),
                list_and_watch_message_receiver.recv(),
            )
            .await
            {
                Ok(message) => {
                    // If receive message to end list_and_watch, send list of unhealthy devices
                    // and shutdown the server by sending message on server_ender_sender channel
                    if message == Ok(ListAndWatchMessageKind::End) {
                        trace!(
                            "InstanceDevicePlugin::list_and_watch - for Instance {} received message to end",
                            dps.instance_name
                        );
                        let devices = prev_virtual_devices
                            .iter()
                            .map(|d| v1beta1::Device {
                                id: d.id.clone(),
                                health: UNHEALTHY.into()
                            })
                            .collect::<Vec<_>>();
                        if !devices.is_empty() {
                            let resp = v1beta1::ListAndWatchResponse { devices };
                            info!(
                                "InstanceDevicePlugin::list_and_watch - for device plugin {}, end response = {:?}",
                                dps.instance_name, resp
                            );
                            kubelet_update_sender.send(Ok(resp))
                                .await
                                .unwrap();
                        }
                        dps.server_ender_sender.clone().send(()).await.unwrap();
                        keep_looping = false;
                    }
                }
                Err(_) => trace!(
                    "InstanceDevicePlugin::list_and_watch - for Instance {} did not receive a message for {} seconds ... continuing", dps.instance_name, polling_interval_secs
                ),
            }
        }
        trace!(
            "InstanceDevicePlugin::list_and_watch - for Instance {} ending",
            dps.instance_name
        );
        // Notify device usage for this instance is gone
        // Best effort, channel may be closed in the case of the Configuration delete
        if let Some(sender) = &dps
            .device_plugin_context
            .read()
            .await
            .usage_update_message_sender
        {
            if let Err(e) = sender.send(ListAndWatchMessageKind::Continue) {
                trace!(
                    "InstanceDevicePlugin::list_and_watch - fails to notify device usage on ending, error {}",
                    e
                );
            }
        }
    }

    /// Called when kubelet is trying to reserve for this node a usage slot (or virtual device) of the Instance.
    /// Tries to update Instance CRD to reserve the requested slot. If cannot reserve that slot, forces `list_and_watch` to continue
    /// (sending kubelet the latest list of slots) and returns error, so kubelet will not schedule the pod to this node.
    async fn allocate(
        &self,
        dps: Arc<DevicePluginService>,
        requests: Request<AllocateRequest>,
        kube_interface: Arc<impl KubeInterface>,
    ) -> Result<Response<AllocateResponse>, Status> {
        let mut container_responses: Vec<v1beta1::ContainerAllocateResponse> = Vec::new();
        // Suffix to add to each device property
        let device_property_suffix = self.instance_id.to_uppercase();

        for request in requests.into_inner().container_requests {
            trace!(
                "InstanceDevicePlugin::allocate - for Instance {} handling request {:?}",
                &dps.instance_name,
                request,
            );
            let mut akri_annotations = HashMap::new();
            let mut akri_device_properties = HashMap::new();
            let mut akri_devices = HashMap::<String, Device>::new();
            for device_usage_id in request.devices_i_ds {
                trace!(
                    "InstanceDevicePlugin::allocate - for Instance {} processing request for device usage slot id {}",
                    &dps.instance_name,
                    device_usage_id
                );

                if let Err(e) = try_update_instance_device_usage(
                    &device_usage_id,
                    &dps.node_name,
                    &dps.instance_name,
                    &dps.config_namespace,
                    DeviceUsageKind::Instance,
                    kube_interface.clone(),
                )
                .await
                {
                    trace!("InstanceDevicePlugin::allocate - could not assign {} slot to {} node ... forcing list_and_watch to continue", device_usage_id, &dps.node_name);
                    dps.list_and_watch_message_sender
                        .send(ListAndWatchMessageKind::Continue)
                        .unwrap();
                    return Err(e);
                }

                let node_usage =
                    NodeUsage::create(&DeviceUsageKind::Instance, &dps.node_name).unwrap();
                akri_annotations.insert(
                    format!("{}{}", AKRI_SLOT_ANNOTATION_NAME_PREFIX, &device_usage_id),
                    node_usage.to_string(),
                );

                // Add suffix _<instance_id> to each device property
                let converted_properties = self
                    .device
                    .properties
                    .iter()
                    .map(|(key, value)| {
                        (
                            format!("{}_{}", key, &device_property_suffix),
                            value.to_string(),
                        )
                    })
                    .collect::<HashMap<String, String>>();
                akri_device_properties.extend(converted_properties);
                akri_devices.insert(dps.instance_name.clone(), self.device.clone());

                trace!(
                    "InstanceDevicePlugin::allocate - finished processing device_usage_id {}",
                    device_usage_id
                );
            }
            // Successfully reserved device_usage_slot[s] for this node.
            // Add response to list of responses
            let broker_properties =
                get_all_broker_properties(&dps.config.broker_properties, &akri_device_properties);
            let response = build_container_allocate_response(
                broker_properties,
                akri_annotations,
                &akri_devices.into_values().collect(),
            );
            container_responses.push(response);
        }

        // Notify device usage had been changed
        if let Some(sender) = &dps
            .device_plugin_context
            .read()
            .await
            .usage_update_message_sender
        {
            if let Err(e) = sender.send(ListAndWatchMessageKind::Continue) {
                error!(
                    "InstanceDevicePlugin::allocate - fails to notify device usage, error {}",
                    e
                );
            }
        }
        trace!(
            "InstanceDevicePlugin::allocate - for Instance {} returning responses",
            &dps.instance_name
        );
        Ok(Response::new(v1beta1::AllocateResponse {
            container_responses,
        }))
    }
}

#[derive(Clone, Default)]
pub struct ConfigurationDevicePlugin {}

impl ConfigurationDevicePlugin {
    async fn list_and_watch(
        &self,
        dps: Arc<DevicePluginService>,
        kube_interface: Arc<impl KubeInterface>,
        kubelet_update_sender: mpsc::Sender<Result<ListAndWatchResponse, Status>>,
        polling_interval_secs: u64,
    ) {
        let mut list_and_watch_message_receiver = dps.list_and_watch_message_sender.subscribe();
        let mut keep_looping = true;
        let mut prev_virtual_devices = HashMap::new();
        while keep_looping {
            trace!(
                "ConfigurationDevicePlugin::list_and_watch - loop iteration for device plugin {}",
                dps.instance_name
            );
            let available_virtual_devices = get_available_virtual_devices(
                &dps.device_plugin_context.read().await.clone().instances,
                &dps.node_name,
                &dps.config_namespace,
                &dps.config.capacity,
                kube_interface.clone(),
            )
            .await;
            info!(
                "ConfigurationDevicePlugin::list_and_watch - available_virtual_devices = {:?}",
                available_virtual_devices
            );

            let current_virtual_devices = available_virtual_devices
                .into_iter()
                .map(|id| (id, HEALTHY.to_string()))
                .collect::<HashMap<String, String>>();

            // Only send the virtual devices if the list has changed
            if current_virtual_devices != prev_virtual_devices {
                // Find devices that no longer exist, set health state to UNHEALTHY for all devices that are gone
                let mut devices_to_report = prev_virtual_devices
                    .keys()
                    .filter(|key| !current_virtual_devices.contains_key(&(*key).clone()))
                    .map(|key| (key.clone(), UNHEALTHY.to_string()))
                    .collect::<HashMap<String, String>>();
                devices_to_report.extend(current_virtual_devices.clone());

                prev_virtual_devices = current_virtual_devices;

                let resp = v1beta1::ListAndWatchResponse {
                    devices: devices_to_report
                        .iter()
                        .map(|(id, health)| v1beta1::Device {
                            id: id.clone(),
                            health: health.clone(),
                        })
                        .collect(),
                };
                info!(
                    "ConfigurationDevicePlugin::list_and_watch - for device plugin {}, response = {:?}",
                    dps.instance_name, resp
                );
                // Send virtual devices list back to kubelet
                if let Err(e) = kubelet_update_sender.send(Ok(resp)).await {
                    trace!(
                            "ConfigurationDevicePlugin::list_and_watch - for device plugin {} kubelet no longer receiving with error {}",
                            dps.instance_name,
                            e
                        );
                    // This means kubelet is down/has been restarted.
                    dps.server_ender_sender.clone().send(()).await.unwrap();
                    keep_looping = false;
                }
            }

            // Sleep for polling_interval_secs unless receive message to shutdown the server
            // or continue (and send another list of devices)
            match timeout(
                Duration::from_secs(polling_interval_secs),
                list_and_watch_message_receiver.recv(),
            )
            .await
            {
                Ok(message) => {
                    // If receive message to end list_and_watch, send list of unhealthy devices
                    // and shutdown the server by sending message on server_ender_sender channel
                    if message == Ok(ListAndWatchMessageKind::End) {
                        trace!(
                            "ConfigurationDevicePlugin::list_and_watch - for device plugin {} received message to end",
                            dps.instance_name
                        );
                        if !prev_virtual_devices.is_empty() {
                            let resp = v1beta1::ListAndWatchResponse {
                                devices: prev_virtual_devices.keys()
                                    .map(|id| v1beta1::Device {
                                        id: id.clone(),
                                        health: UNHEALTHY.to_string(),
                                    })
                                    .collect(),
                            };
                            info!(
                                "ConfigurationDevicePlugin::list_and_watch - for device plugin {}, end response = {:?}",
                                dps.instance_name, resp
                            );
                            kubelet_update_sender.send(Ok(resp))
                                .await
                                .unwrap();
                        }
                        dps.server_ender_sender.clone().send(()).await.unwrap();
                        keep_looping = false;
                    }
                },
                Err(_) => trace!(
                    "ConfigurationDevicePlugin::list_and_watch - for device plugin {} did not receive a message for {} seconds ... continuing",
                     dps.instance_name, polling_interval_secs
                ),
            }
        }
        trace!(
            "ConfigurationDevicePlugin::list_and_watch - for device plugin {} ending",
            dps.instance_name
        );
    }

    /// Called when kubelet is trying to reserve for this node a usage slot (or virtual device) of the Instance.
    /// Tries to update Instance CRD to reserve the requested slot. If cannot reserve that slot, forces `list_and_watch` to continue
    /// (sending kubelet the latest list of slots) and returns error, so kubelet will not schedule the pod to this node.
    async fn allocate(
        &self,
        dps: Arc<DevicePluginService>,
        requests: Request<AllocateRequest>,
        kube_interface: Arc<impl KubeInterface>,
    ) -> Result<Response<AllocateResponse>, Status> {
        let mut container_responses = Vec::new();
        let mut allocated_instances = HashMap::<String, (String, String)>::new();
        for request in requests.into_inner().container_requests {
            trace!(
                "ConfigurationDevicePlugin::allocate - for device plugin {} handling request {:?}",
                &dps.instance_name,
                request,
            );

            let resources_to_allocate = match get_virtual_device_resources(
                request.devices_i_ds.clone(),
                &dps.device_plugin_context.read().await.clone().instances,
                &dps.node_name,
                &dps.config_namespace,
                &dps.config.capacity,
                kube_interface.clone(),
            )
            .await
            {
                Ok(resources) => {
                    info!(
                        "ConfigurationDevicePlugin::allocate - resource to allocate = {:?}",
                        resources
                    );
                    resources
                }
                Err(e) => {
                    dps.list_and_watch_message_sender
                        .send(ListAndWatchMessageKind::Continue)
                        .unwrap();
                    return Err(e);
                }
            };

            let mut akri_annotations = HashMap::new();
            let mut akri_device_properties = HashMap::new();
            let mut akri_devices = HashMap::<String, Device>::new();
            for (vdev_id, (instance_name, device_usage_id)) in resources_to_allocate {
                info!(
                        "ConfigurationDevicePlugin::allocate - for device plugin {} processing request for vdevice id {}",
                        &dps.instance_name,
                        vdev_id
                    );
                // Find device from instance_map
                let (device, instance_id) = match dps
                    .device_plugin_context
                    .read()
                    .await
                    .instances
                    .get(&instance_name)
                    .ok_or_else(|| {
                        Status::new(
                            Code::Unknown,
                            format!("instance {} not found in instance map", instance_name),
                        )
                    }) {
                    Ok(instance_info) => (
                        instance_info.device.clone(),
                        instance_info.instance_id.clone(),
                    ),
                    Err(e) => {
                        dps.list_and_watch_message_sender
                            .send(ListAndWatchMessageKind::Continue)
                            .unwrap();
                        return Err(e);
                    }
                };

                if let Err(e) = try_update_instance_device_usage(
                    &device_usage_id,
                    &dps.node_name,
                    &instance_name,
                    &dps.config_namespace,
                    DeviceUsageKind::Configuration(vdev_id.clone()),
                    kube_interface.clone(),
                )
                .await
                {
                    trace!("ConfigurationDevicePlugin::allocate - could not assign {} slot to {} node ... forcing list_and_watch to continue", device_usage_id, &dps.node_name);
                    dps.list_and_watch_message_sender
                        .send(ListAndWatchMessageKind::Continue)
                        .unwrap();
                    return Err(e);
                }

                let node_usage = NodeUsage::create(
                    &DeviceUsageKind::Configuration(vdev_id.clone()),
                    &dps.node_name,
                )
                .unwrap();
                akri_annotations.insert(
                    format!("{}{}", AKRI_SLOT_ANNOTATION_NAME_PREFIX, &device_usage_id),
                    node_usage.to_string(),
                );

                // Add suffix _<instance_id> to each device property
                let device_property_suffix = instance_id.to_uppercase();
                let converted_properties = device
                    .properties
                    .iter()
                    .map(|(key, value)| {
                        (
                            format!("{}_{}", key, &device_property_suffix),
                            value.to_string(),
                        )
                    })
                    .collect::<HashMap<String, String>>();
                akri_device_properties.extend(converted_properties);
                akri_devices.insert(instance_name.clone(), device.clone());

                allocated_instances.insert(
                    instance_name.clone(),
                    (instance_name.clone(), device_usage_id.clone()),
                );

                trace!(
                    "ConfigurationDevicePlugin::allocate - finished processing device_usage_id {}",
                    device_usage_id
                );
            }
            // Successfully reserved device_usage_slot[s] for this node.
            // Add response to list of responses
            let broker_properties =
                get_all_broker_properties(&dps.config.broker_properties, &akri_device_properties);
            let response = build_container_allocate_response(
                broker_properties,
                akri_annotations,
                &akri_devices.into_values().collect(),
            );
            container_responses.push(response);
        }

        // Notify effected instance device plugin to rescan list and watch and update the cl_usage_slot
        {
            let device_plugin_context = dps.device_plugin_context.read().await;
            for instance_name in allocated_instances.keys() {
                trace!(
                    "ConfigurationDevicePlugin::allocate - notify Instance {} to refresh list_and_watch",
                    instance_name,
                );

                if let Some(instance_info) = device_plugin_context.instances.get(instance_name) {
                    instance_info
                        .list_and_watch_message_sender
                        .send(ListAndWatchMessageKind::Continue)
                        .unwrap();
                }
            }
        }
        trace!(
            "ConfigurationDevicePlugin::allocate - for device plugin {} returning responses",
            &dps.instance_name
        );
        Ok(Response::new(v1beta1::AllocateResponse {
            container_responses,
        }))
    }
}

async fn get_available_virtual_devices(
    instances: &HashMap<String, InstanceInfo>,
    node_name: &str,
    instance_namespace: &str,
    capacity: &i32,
    kube_interface: Arc<impl KubeInterface>,
) -> HashSet<String> {
    let mut instance_device_usage_states = HashMap::new();
    for instance_name in instances.keys() {
        let device_usage_states = get_instance_device_usage_states(
            node_name,
            instance_name,
            instance_namespace,
            capacity,
            kube_interface.clone(),
        )
        .await;
        instance_device_usage_states.insert(instance_name.to_string(), device_usage_states);
    }
    let mut vdev_ids_to_report = HashSet::new();
    let mut free_instances = HashSet::new();
    for (instance_name, device_usage_states) in instance_device_usage_states {
        for (_id, state) in device_usage_states {
            match state {
                DeviceUsageStatus::Free => {
                    free_instances.insert(instance_name.clone());
                }
                DeviceUsageStatus::ReservedByConfiguration(vdev_id) => {
                    vdev_ids_to_report.insert(vdev_id);
                }
                _ => (),
            };
        }
    }

    // Decide virtual device id, report already reserved virtual devices + 1 free device usage (if available) per instance
    let mut id_to_set: u32 = 0;
    for _x in 0..free_instances.len() {
        while vdev_ids_to_report.get(&format!("{}", id_to_set)).is_some() {
            id_to_set += 1;
        }
        let id_string = format!("{}", id_to_set);
        vdev_ids_to_report.insert(id_string);
        id_to_set += 1;
    }

    vdev_ids_to_report
}

async fn get_virtual_device_resources(
    requested_vdev_ids: Vec<String>,
    instances: &HashMap<String, InstanceInfo>,
    node_name: &str,
    instance_namespace: &str,
    capacity: &i32,
    kube_interface: Arc<impl KubeInterface>,
) -> Result<HashMap<String, (String, String)>, Status> {
    let mut instance_device_usage_states = HashMap::new();
    for instance_name in instances.keys() {
        let device_usage_states = get_instance_device_usage_states(
            node_name,
            instance_name,
            instance_namespace,
            capacity,
            kube_interface.clone(),
        )
        .await;
        instance_device_usage_states.insert(instance_name.to_string(), device_usage_states);
    }

    let mut usage_ids_to_use = HashMap::<String, (String, String)>::new();

    // Get all available virtual devices, group by device usage status,
    // a virtual device is available if it's Free or ReservedByConfiguration with requested vdev_ids
    let mut free_device_usage_states = HashMap::new();
    let mut reserved_by_configuration_usage_states = HashMap::new();
    for (instance_name, device_usage_state) in instance_device_usage_states {
        for (slot, state) in device_usage_state {
            match state {
                DeviceUsageStatus::Free => {
                    free_device_usage_states
                        .entry(instance_name.to_string())
                        .or_insert(HashSet::new())
                        .insert(slot);
                }
                DeviceUsageStatus::ReservedByConfiguration(vdev_id) => {
                    if requested_vdev_ids.contains(&vdev_id) {
                        reserved_by_configuration_usage_states
                            .entry(instance_name.to_string())
                            .or_insert(HashMap::new())
                            .insert(slot, vdev_id);
                    }
                }

                _ => (),
            };
        }
    }

    // Find (Instance, usage slot) for vdev ids, (Instance, usage slot) in free_device_usage_states and
    // reserved_by_configuration_usage_states are available to be assigned.
    // Iterate vdev ids to look up vdev id from reserved_by_configuration_usage_states, if not found
    // pick one (Instance, usage slot) from free_device_usage_states
    let unallocated_device_ids = requested_vdev_ids
        .into_iter()
        .filter(|vdev_id| {
            // true means not allocated
            let mut resource: Option<(String, String)> = None;

            for (instance_name, slots) in &reserved_by_configuration_usage_states {
                for (slot, vid) in slots {
                    if vdev_id == vid {
                        resource = Some((instance_name.clone(), slot.clone()));
                        break;
                    }
                }
                if resource.is_some() {
                    break;
                }
            }

            if resource.is_none() {
                if let Some((instance_name, slots)) = &free_device_usage_states
                    .iter()
                    .max_by(|a, b| a.1.len().cmp(&b.1.len()))
                {
                    if let Some(slot) = slots.iter().next() {
                        resource = Some((instance_name.to_string(), slot.to_string()));
                    }
                }
            }

            match resource {
                Some((instance_name, slot)) => {
                    reserved_by_configuration_usage_states.remove(&instance_name);
                    free_device_usage_states.remove(&instance_name);
                    usage_ids_to_use.insert(vdev_id.clone(), (instance_name, slot));
                    false
                }
                None => true,
            }
        })
        .collect::<Vec<String>>();

    // Return error if any unallocated vdev id
    if !unallocated_device_ids.is_empty() {
        return Err(Status::new(
            Code::Unknown,
            "Insufficient instances to allocate",
        ));
    }
    Ok(usage_ids_to_use)
}

/// This returns device usage status of all slots for an Instance on a given node
/// if the Instance doesn't exist or fail to parse device usage of its slots return
///  DeviceUsageStatus::Unknown since insufficient information to decide the usage state
pub async fn get_instance_device_usage_states(
    node_name: &str,
    instance_name: &str,
    instance_namespace: &str,
    capacity: &i32,
    kube_interface: Arc<impl KubeInterface>,
) -> Vec<(String, DeviceUsageStatus)> {
    let mut device_usage_states = Vec::new();
    match kube_interface
        .find_instance(instance_name, instance_namespace)
        .await
    {
        Ok(kube_akri_instance) => {
            for (device_name, device_usage_string) in kube_akri_instance.spec.device_usage {
                let device_usage_status = match NodeUsage::from_str(&device_usage_string) {
                    Ok(node_usage) => get_device_usage_state(&node_usage, node_name),
                    Err(_) => {
                        error!(
                            "get_instance_device_usage_states - fail to parse device usage {}",
                            device_usage_string
                        );
                        DeviceUsageStatus::Unknown
                    }
                };
                device_usage_states.push((device_name.clone(), device_usage_status));
            }
            device_usage_states
        }
        Err(_) => (0..*capacity)
            .map(|x| {
                (
                    format!("{}-{}", instance_name, x),
                    DeviceUsageStatus::Unknown,
                )
            })
            .collect(),
    }
}

/// This returns device usage status of a `device_usage_id` slot for an instance on a given node
/// # More details
/// Cases based on the device usage value
/// 1. DeviceUsageKind::Free ... this means that the device is available for use
///     * (ACTION) return DeviceUsageStatus::Free
/// 2. node_usage.node_name == node_name ... this means node_name previously used device_usage
///     * (ACTION) return previously reserved kind, DeviceUsageStatus::ReservedByConfiguration or DeviceUsageStatus::ReservedByInstance
/// 3. node_usage.node_name == (some other node) ... this means that we believe this device is in use by another node
///     * (ACTION) return DeviceUsageStatus::ReservedByOtherNode
fn get_device_usage_state(node_usage: &NodeUsage, node_name: &str) -> DeviceUsageStatus {
    let device_usage_state = match node_usage.get_kind() {
        DeviceUsageKind::Free => DeviceUsageStatus::Free,
        DeviceUsageKind::Configuration(vdev_id) => {
            DeviceUsageStatus::ReservedByConfiguration(vdev_id)
        }
        DeviceUsageKind::Instance => DeviceUsageStatus::ReservedByInstance,
    };
    if device_usage_state != DeviceUsageStatus::Free && !node_usage.is_same_node(node_name) {
        return DeviceUsageStatus::ReservedByOtherNode;
    }
    device_usage_state
}

/// This tries up to `MAX_INSTANCE_UPDATE_TRIES` to update the requested slot of the Instance with the this node's name.
/// It cannot be assumed that this will successfully update Instance on first try since Device Plugins on other nodes
/// may be simultaneously trying to update the Instance.
/// This returns an error if slot already be reserved by other nodes or device plugins,
/// cannot be updated or `MAX_INSTANCE_UPDATE_TRIES` attempted.
async fn try_update_instance_device_usage(
    device_usage_id: &str,
    node_name: &str,
    instance_name: &str,
    instance_namespace: &str,
    desired_device_usage_kind: DeviceUsageKind,
    kube_interface: Arc<impl KubeInterface>,
) -> Result<(), Status> {
    let mut instance: InstanceSpec;
    for x in 0..MAX_INSTANCE_UPDATE_TRIES {
        // Grab latest instance
        match kube_interface
            .find_instance(instance_name, instance_namespace)
            .await
        {
            Ok(instance_object) => instance = instance_object.spec,
            Err(_) => {
                trace!(
                    "try_update_instance_device_usage - could not find Instance {}",
                    instance_name
                );
                return Err(Status::new(
                    Code::Unknown,
                    format!("Could not find Instance {}", instance_name),
                ));
            }
        }

        // Update the instance to reserve this slot for this node iff it is available and not already reserved for this node.
        let current_device_usage_string = instance.device_usage.get(device_usage_id);
        if current_device_usage_string.is_none() {
            // No corresponding id found
            trace!(
                "try_update_instance_device_usage - could not find {} id in device_usage",
                device_usage_id
            );
            return Err(Status::new(
                Code::Unknown,
                "Could not find device usage slot",
            ));
        }

        let current_device_usage = NodeUsage::from_str(current_device_usage_string.unwrap())
            .map_err(|_| {
                Status::new(
                    Code::Unknown,
                    format!(
                        "Fails to parse {} to DeviceUsage ",
                        current_device_usage_string.unwrap()
                    ),
                )
            })?;
        // Call get_device_usage_state to check current device usage to see if the slot can be reserved.
        // A device usage slot can be reserved if it's free or already reserved by this node and the desired usage kind matches.
        // For slots owned by this node, get_device_usage_state returns ReservedByConfiguration or ReservedByInstance.
        // For slots owned by other nodes (by Configuration or Instance), get_device_usage_state returns ReservedByOtherNode.
        match get_device_usage_state(&current_device_usage, node_name) {
            DeviceUsageStatus::Free => {
                let new_device_usage = NodeUsage::create(&desired_device_usage_kind, node_name)
                    .map_err(|e| {
                        Status::new(
                            Code::Unknown,
                            format!("Fails to create DeviceUsage - {}", e),
                        )
                    })?;
                instance
                    .device_usage
                    .insert(device_usage_id.to_string(), new_device_usage.to_string());

                if let Err(e) = kube_interface
                    .update_instance(&instance, instance_name, instance_namespace)
                    .await
                {
                    if x == (MAX_INSTANCE_UPDATE_TRIES - 1) {
                        trace!("try_update_instance_device_usage - update_instance returned error [{}] after max tries ... returning error", e);
                        return Err(Status::new(Code::Unknown, "Could not update Instance"));
                    }
                    random_delay().await;
                } else {
                    return Ok(());
                }
            }
            DeviceUsageStatus::ReservedByConfiguration(_) => {
                if matches!(desired_device_usage_kind, DeviceUsageKind::Configuration(_)) {
                    return Ok(());
                } else {
                    return Err(Status::new(
                        Code::Unknown,
                        format!(
                            "Requested device kind {:?} already in use by DeviceUsageKind::Configuration",
                            desired_device_usage_kind,
                        ),
                    ));
                }
            }
            DeviceUsageStatus::ReservedByInstance => {
                if matches!(desired_device_usage_kind, DeviceUsageKind::Instance) {
                    return Ok(());
                } else {
                    return Err(Status::new(
                        Code::Unknown,
                        format!(
                            "Requested device kind {:?} already in use by DeviceUsageKind::Instance",
                            desired_device_usage_kind,
                        ),
                    ));
                }
            }
            DeviceUsageStatus::ReservedByOtherNode => {
                trace!("try_update_instance_device_usage - request for device slot {} previously claimed by a diff node {} than this one {} ... indicates the device on THIS node must be marked unhealthy, invoking ListAndWatch ... returning failure, next scheduling should succeed!",
                    device_usage_id, current_device_usage.get_node_name(), node_name);
                return Err(Status::new(
                    Code::Unknown,
                    format!(
                        "Requested device kind {:?} already in use by other nodes",
                        desired_device_usage_kind,
                    ),
                ));
            }
            DeviceUsageStatus::Unknown => {
                trace!(
                    "try_update_instance_device_usage - request for device slot {} status unknown!",
                    device_usage_id
                );
                return Err(Status::new(
                    Code::Unknown,
                    "Requested device usage status unknown",
                ));
            }
        };
    }
    Ok(())
}

/// This sets the volume mounts and environment variables according to the instance's `DiscoveryHandler`.
fn build_container_allocate_response(
    broker_properties: HashMap<String, String>,
    annotations: HashMap<String, String>,
    devices: &Vec<Device>,
) -> v1beta1::ContainerAllocateResponse {
    let mut total_mounts = Vec::new();
    let mut total_device_specs = Vec::new();
    for device in devices {
        // Cast v0 discovery Mount and DeviceSpec types to v1beta1 DevicePlugin types
        let mounts: Vec<Mount> = device
            .mounts
            .clone()
            .into_iter()
            .map(|mount| Mount {
                container_path: mount.container_path,
                host_path: mount.host_path,
                read_only: mount.read_only,
            })
            .collect();
        total_mounts.extend(mounts);

        let device_specs: Vec<DeviceSpec> = device
            .device_specs
            .clone()
            .into_iter()
            .map(|device_spec| DeviceSpec {
                container_path: device_spec.container_path,
                host_path: device_spec.host_path,
                permissions: device_spec.permissions,
            })
            .collect();
        total_device_specs.extend(device_specs);
    }
    // Create response, setting environment variables to be an instance's properties.
    v1beta1::ContainerAllocateResponse {
        annotations,
        mounts: total_mounts,
        devices: total_device_specs,
        envs: broker_properties,
    }
}

/// Try to find Instance CRD for this instance or create one and add it to the global InstanceMap
/// If a Config does not exist for this instance, return error.
/// This is most likely caused by deletion of a Config right after adding it, in which case
/// `handle_config_delete` fails to delete this instance because kubelet has yet to call `list_and_watch`
async fn try_create_instance(
    dps: Arc<DevicePluginService>,
    instance_dp: &InstanceDevicePlugin,
    kube_interface: Arc<impl KubeInterface>,
) -> Result<(), anyhow::Error> {
    // Make sure Configuration exists for instance
    if let Err(e) = kube_interface
        .find_configuration(&dps.config_name, &dps.config_namespace)
        .await
    {
        error!(
            "try_create_instance - no Configuration for device {} ... returning error",
            dps.instance_name
        );
        return Err(e);
    }

    let device_usage: std::collections::HashMap<String, String> = (0..dps.config.capacity)
        .map(|x| {
            (
                format!("{}-{}", dps.instance_name, x),
                NodeUsage::default().to_string(),
            )
        })
        .collect();
    let instance = InstanceSpec {
        configuration_name: dps.config_name.clone(),
        shared: instance_dp.shared,
        nodes: vec![dps.node_name.clone()],
        device_usage,
        broker_properties: get_all_broker_properties(
            &dps.config.broker_properties,
            &instance_dp.device.properties,
        ),
    };

    // Try up to MAX_INSTANCE_UPDATE_TRIES to create or update instance, breaking on success
    for x in 0..MAX_INSTANCE_UPDATE_TRIES {
        // First check if instance already exists
        match kube_interface
            .find_instance(&dps.instance_name, &dps.config_namespace)
            .await
        {
            Ok(mut instance_object) => {
                trace!(
                    "try_create_instance - discovered Instance {} already created",
                    dps.instance_name
                );

                // Check if instance's node list already contains this node, possibly due to device plugin failure and restart
                if !instance_object.spec.nodes.contains(&dps.node_name) {
                    instance_object.spec.nodes.push(dps.node_name.clone());
                    match kube_interface
                        .update_instance(
                            &instance_object.spec,
                            &instance_object.metadata.name.unwrap(),
                            &dps.config_namespace,
                        )
                        .await
                    {
                        Ok(()) => {
                            trace!(
                                "try_create_instance - updated Instance {} to include {}",
                                dps.instance_name,
                                dps.node_name
                            );
                            break;
                        }
                        Err(e) => {
                            trace!("try_create_instance - call to update_instance returned with error {} on try # {} of {}", e, x, MAX_INSTANCE_UPDATE_TRIES);
                            if x == (MAX_INSTANCE_UPDATE_TRIES - 1) {
                                return Err(e);
                            }
                        }
                    };
                } else {
                    break;
                }
            }
            Err(_) => {
                match kube_interface
                    .create_instance(
                        &instance,
                        &dps.instance_name,
                        &dps.config_namespace,
                        &dps.config_name,
                        &dps.config_uid,
                    )
                    .await
                {
                    Ok(()) => {
                        trace!(
                            "try_create_instance - created Instance with name {}",
                            dps.instance_name
                        );
                        break;
                    }
                    Err(e) => {
                        trace!("try_create_instance - couldn't create instance with error {} on try # {} of {}", e, x, MAX_INSTANCE_UPDATE_TRIES);
                        if x == MAX_INSTANCE_UPDATE_TRIES - 1 {
                            return Err(e);
                        }
                    }
                }
            }
        }
        random_delay().await;
    }

    // Successfully created or updated instance. Add it to instance_map.
    dps.device_plugin_context.write().await.instances.insert(
        dps.instance_name.clone(),
        InstanceInfo {
            list_and_watch_message_sender: dps.list_and_watch_message_sender.clone(),
            connectivity_status: InstanceConnectivityStatus::Online,
            instance_id: instance_dp.instance_id.clone(),
            device: instance_dp.device.clone(),
        },
    );

    Ok(())
}

/// This sends message to end `list_and_watch` and removes instance from InstanceMap.
/// Called when an instance has been offline for too long.
pub async fn terminate_device_plugin_service(
    instance_name: &str,
    device_plugin_context: Arc<RwLock<DevicePluginContext>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    let mut device_plugin_context = device_plugin_context.write().await;
    info!(
        "terminate_device_plugin_service -- forcing list_and_watch to end for Instance {}",
        instance_name
    );
    device_plugin_context
        .instances
        .get(instance_name)
        .unwrap()
        .list_and_watch_message_sender
        .send(ListAndWatchMessageKind::End)
        .unwrap();

    trace!(
        "terminate_device_plugin_service -- removing Instance {} from instance_map",
        instance_name
    );
    device_plugin_context.instances.remove(instance_name);
    Ok(())
}

/// This creates a Configuration's unique name
pub fn get_device_configuration_name(config_name: &str) -> String {
    config_name.to_string().replace(['.', '/'], "-")
}

/// This creates an Instance's unique name
pub fn get_device_instance_name(id: &str, config_name: &str) -> String {
    format!("{}-{}", config_name, &id)
        .replace('.', "-")
        .replace('/', "-")
}

// Aggregate a Configuration and Device's properties so they can be displayed in an Instance and injected into brokers as environment variables.
pub fn get_all_broker_properties(
    configuration_properties: &HashMap<String, String>,
    device_properties: &HashMap<String, String>,
) -> HashMap<String, String> {
    configuration_properties
        .clone()
        .into_iter()
        .chain(device_properties.clone())
        .collect::<HashMap<String, String>>()
}

#[cfg(test)]
mod device_plugin_service_tests {
    use super::*;
    use akri_shared::akri::configuration::Configuration;
    use akri_shared::{
        akri::instance::{Instance, InstanceSpec},
        k8s::MockKubeInterface,
    };
    use std::{
        fs,
        io::{Error, ErrorKind},
    };

    enum NodeName {
        ThisNode,
        OtherNode,
    }

    enum DevicePluginKind {
        Configuration,
        Instance,
    }

    // Need to be kept alive during tests
    struct DevicePluginServiceReceivers {
        configuration_list_and_watch_message_receiver: broadcast::Receiver<ListAndWatchMessageKind>,
        instance_list_and_watch_message_receiver: broadcast::Receiver<ListAndWatchMessageKind>,
    }

    fn configure_find_instance(
        mock: &mut MockKubeInterface,
        result_file: &'static str,
        instance_name: String,
        instance_namespace: String,
        device_usage_node: String,
        node_name: NodeName,
        expected_calls: usize,
    ) {
        let instance_name_clone = instance_name.clone();
        mock.expect_find_instance()
            .times(expected_calls)
            .withf(move |name: &str, namespace: &str| {
                namespace == instance_namespace && name == instance_name
            })
            .returning(move |_, _| {
                let mut instance_json =
                    fs::read_to_string(result_file).expect("Unable to read file");
                let host_name = match node_name {
                    NodeName::ThisNode => "node-a",
                    NodeName::OtherNode => "other",
                };
                instance_json = instance_json.replace("node-a", host_name);
                instance_json = instance_json.replace("config-a-b494b6", &instance_name_clone);
                instance_json =
                    instance_json.replace("\":\"\"", &format!("\":\"{}\"", device_usage_node));
                let instance: Instance = serde_json::from_str(&instance_json).unwrap();
                Ok(instance)
            });
    }

    fn setup_find_instance_with_mock_instances(
        mock: &mut MockKubeInterface,
        instance_namespace: &str,
        mock_instances: Vec<(String, Instance)>,
    ) {
        for (instance_name, kube_instance) in mock_instances {
            let instance_namespace = instance_namespace.to_string();
            mock.expect_find_instance()
                .times(1)
                .withf(move |name: &str, namespace: &str| {
                    namespace == instance_namespace && name == instance_name
                })
                .returning(move |_, _| Ok(kube_instance.clone()));
        }
    }

    fn setup_find_instance_with_not_found_err(
        mock: &mut MockKubeInterface,
        instance_name: &str,
        instance_namespace: &str,
    ) {
        let instance_name = instance_name.to_string();
        let instance_namespace = instance_namespace.to_string();
        mock.expect_find_instance()
            .times(1)
            .withf(move |name: &str, namespace: &str| {
                namespace == instance_namespace && name == instance_name
            })
            .returning(move |_, _| Err(get_kube_not_found_error().into()));
    }

    fn create_device_plugin_service(
        device_plugin_kind: DevicePluginKind,
        connectivity_status: InstanceConnectivityStatus,
        add_to_instance_map: bool,
    ) -> (DevicePluginService, DevicePluginServiceReceivers) {
        let path_to_config = "../test/yaml/config-a.yaml";
        let instance_id = "b494b6";
        let kube_akri_config_yaml =
            fs::read_to_string(path_to_config).expect("Unable to read file");
        let kube_akri_config: Configuration = serde_yaml::from_str(&kube_akri_config_yaml).unwrap();
        let config_name = kube_akri_config.metadata.name.as_ref().unwrap();
        let device_instance_name = get_device_instance_name(instance_id, config_name);
        let (
            configuration_list_and_watch_message_sender,
            configuration_list_and_watch_message_receiver,
        ) = broadcast::channel(4);
        let (instance_list_and_watch_message_sender, instance_list_and_watch_message_receiver) =
            broadcast::channel(4);
        let (server_ender_sender, _) = mpsc::channel(1);

        let device = Device {
            id: "n/a".to_string(),
            properties: HashMap::from([(
                "DEVICE_LOCATION_INFO".to_string(),
                "endpoint".to_string(),
            )]),
            mounts: Vec::new(),
            device_specs: Vec::new(),
        };
        let mut instances = HashMap::new();
        if add_to_instance_map {
            let instance_info: InstanceInfo = InstanceInfo {
                list_and_watch_message_sender: instance_list_and_watch_message_sender.clone(),
                connectivity_status,
                instance_id: instance_id.to_string(),
                device: device.clone(),
            };
            instances.insert(device_instance_name.clone(), instance_info);
        }
        let device_plugin_context = Arc::new(RwLock::new(DevicePluginContext {
            usage_update_message_sender: Some(configuration_list_and_watch_message_sender.clone()),
            instances,
        }));

        let (list_and_watch_message_sender, device_plugin_behavior) = match device_plugin_kind {
            DevicePluginKind::Instance => (
                instance_list_and_watch_message_sender,
                DevicePluginBehavior::Instance(InstanceDevicePlugin {
                    instance_id: instance_id.to_string(),
                    shared: false,
                    device,
                }),
            ),
            DevicePluginKind::Configuration => (
                configuration_list_and_watch_message_sender,
                DevicePluginBehavior::Configuration(ConfigurationDevicePlugin::default()),
            ),
        };
        let dps = DevicePluginService {
            instance_name: device_instance_name,
            config: kube_akri_config.spec.clone(),
            config_name: config_name.to_string(),
            config_uid: kube_akri_config.metadata.uid.unwrap(),
            config_namespace: kube_akri_config.metadata.namespace.unwrap(),
            node_name: "node-a".to_string(),
            device_plugin_context,
            list_and_watch_message_sender,
            server_ender_sender,
            device_plugin_behavior,
        };
        (
            dps,
            DevicePluginServiceReceivers {
                configuration_list_and_watch_message_receiver,
                instance_list_and_watch_message_receiver,
            },
        )
    }

    fn get_kube_not_found_error() -> kube::Error {
        // Mock error thrown when instance not found
        kube::Error::Api(kube::error::ErrorResponse {
            status: "Failure".to_string(),
            message: "instances.akri.sh \"akri-blah-901a7b\" not found".to_string(),
            reason: "NotFound".to_string(),
            code: k8s::ERROR_NOT_FOUND,
        })
    }

    // Tests that configuration device plugin names are formatted correctly
    #[test]
    fn test_get_device_configuration_name() {
        let names_to_test = [
            ("no_dash_no_dot", "no_dash_no_dot"),
            ("usb/camera", "usb-camera"),
            ("another//camera", "another--camera"),
            ("name.with.dot", "name-with-dot"),
            ("name.with..dots...", "name-with--dots---"),
        ];
        names_to_test.iter().for_each(|(test, expected)| {
            println!("{:?}", (test, expected));
            assert_eq!(get_device_configuration_name(test), expected.to_string());
        });
    }

    // Tests that instance names are formatted correctly
    #[test]
    fn test_get_device_instance_name() {
        let instance_name1: String = "/dev/video0".to_string();
        let instance_name2: String = "10.1.2.3".to_string();
        assert_eq!(
            "usb-camera--dev-video0",
            get_device_instance_name(&instance_name1, "usb-camera")
        );
        assert_eq!(
            "ip-camera-10-1-2-3".to_string(),
            get_device_instance_name(&instance_name2, "ip-camera")
        );
    }

    // Test that a Device and Configuration's properties are aggregated and that
    // a Device property overwrites a Configuration's.
    #[test]
    fn test_get_all_broker_properties() {
        let mut device_properties = HashMap::new();
        device_properties.insert("ENDPOINT".to_string(), "123".to_string());
        device_properties.insert("OVERWRITE".to_string(), "222".to_string());
        let mut configuration_properties = HashMap::new();
        configuration_properties.insert("USE HD".to_string(), "true".to_string());
        configuration_properties.insert("OVERWRITE".to_string(), "111".to_string());
        let all_properties =
            get_all_broker_properties(&configuration_properties, &device_properties);
        assert_eq!(all_properties.len(), 3);
        assert_eq!(all_properties.get("ENDPOINT").unwrap(), "123");
        assert_eq!(all_properties.get("USE HD").unwrap(), "true");
        assert_eq!(all_properties.get("OVERWRITE").unwrap(), "222");
    }

    // Test correct device usage status is returned when a device usage slot is used on the same node
    #[test]
    fn test_get_device_usage_state_same_node() {
        let _ = env_logger::builder().is_test(true).try_init();
        let this_node = "node-a";
        let vdev_id = "vdev_0";
        // Free
        assert_eq!(
            get_device_usage_state(
                &NodeUsage::create(&DeviceUsageKind::Free, "").unwrap(),
                this_node
            ),
            DeviceUsageStatus::Free
        );
        // Used by Configuration
        assert_eq!(
            get_device_usage_state(
                &NodeUsage::create(
                    &DeviceUsageKind::Configuration(vdev_id.to_string()),
                    this_node
                )
                .unwrap(),
                this_node
            ),
            DeviceUsageStatus::ReservedByConfiguration(vdev_id.to_string())
        );
        // Used by Instance
        assert_eq!(
            get_device_usage_state(
                &NodeUsage::create(&DeviceUsageKind::Instance, this_node).unwrap(),
                this_node
            ),
            DeviceUsageStatus::ReservedByInstance
        );
    }

    // Test DeviceUsageStatus::ReservedByOtherNode is returned when a device usage slot is used on a different node
    #[test]
    fn test_get_device_usage_state_different_node() {
        let _ = env_logger::builder().is_test(true).try_init();
        let this_node = "node-a";
        let that_node = "node-b";
        let vdev_id = "vdev_0";
        // Free
        assert_eq!(
            get_device_usage_state(
                &NodeUsage::create(&DeviceUsageKind::Free, "").unwrap(),
                this_node
            ),
            DeviceUsageStatus::Free
        );
        // Used by Configuration
        assert_eq!(
            get_device_usage_state(
                &NodeUsage::create(
                    &DeviceUsageKind::Configuration(vdev_id.to_string()),
                    that_node
                )
                .unwrap(),
                this_node
            ),
            DeviceUsageStatus::ReservedByOtherNode
        );
        // Used by Instance
        assert_eq!(
            get_device_usage_state(
                &NodeUsage::create(&DeviceUsageKind::Instance, that_node).unwrap(),
                this_node
            ),
            DeviceUsageStatus::ReservedByOtherNode
        );
    }

    fn configure_find_configuration(
        mock: &mut MockKubeInterface,
        config_name: String,
        config_namespace: String,
    ) {
        mock.expect_find_configuration()
            .times(1)
            .withf(move |name: &str, namespace: &str| {
                namespace == config_namespace && name == config_name
            })
            .returning(move |_, _| {
                let path_to_config = "../test/yaml/config-a.yaml";
                let kube_akri_config_yaml =
                    fs::read_to_string(path_to_config).expect("Unable to read file");
                let kube_akri_config: Configuration =
                    serde_yaml::from_str(&kube_akri_config_yaml).unwrap();
                Ok(kube_akri_config)
            });
    }

    // Tests that try_create_instance creates an instance
    #[tokio::test]
    async fn test_try_create_instance() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (device_plugin_service, _device_plugin_service_receivers) =
            create_device_plugin_service(
                DevicePluginKind::Instance,
                InstanceConnectivityStatus::Online,
                false,
            );
        let mut mock = MockKubeInterface::new();
        configure_find_configuration(
            &mut mock,
            device_plugin_service.config_name.clone(),
            device_plugin_service.config_namespace.clone(),
        );
        let instance_name = device_plugin_service.instance_name.clone();
        let config_name = device_plugin_service.config_name.clone();
        let config_uid = device_plugin_service.config_uid.clone();
        let config_namespace = device_plugin_service.config_namespace.clone();
        mock.expect_find_instance()
            .times(1)
            .withf(move |name: &str, namespace: &str| {
                namespace == config_namespace && name == instance_name
            })
            .returning(move |_, _| Err(get_kube_not_found_error().into()));
        let instance_name = device_plugin_service.instance_name.clone();
        let config_namespace = device_plugin_service.config_namespace.clone();
        mock.expect_create_instance()
            .withf(move |instance, name, namespace, owner_name, owner_uid| {
                namespace == config_namespace
                    && name == instance_name
                    && instance.nodes.contains(&"node-a".to_string())
                    && owner_name == config_name
                    && owner_uid == config_uid
            })
            .returning(move |_, _, _, _, _| Ok(()));

        let dps = Arc::new(device_plugin_service);
        if let DevicePluginBehavior::Instance(instance_device_plugin) = &dps.device_plugin_behavior
        {
            assert!(
                try_create_instance(dps.clone(), instance_device_plugin, Arc::new(mock))
                    .await
                    .is_ok()
            );
            assert!(dps
                .device_plugin_context
                .read()
                .await
                .instances
                .contains_key(&dps.instance_name));
        } else {
            panic!("Incorrect device plugin kind configured");
        }
    }

    // Tests that try_create_instance updates already existing instance with this node
    #[tokio::test]
    async fn test_try_create_instance_already_created() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (device_plugin_service, _device_plugin_service_receivers) =
            create_device_plugin_service(
                DevicePluginKind::Instance,
                InstanceConnectivityStatus::Online,
                false,
            );
        let mut mock = MockKubeInterface::new();
        configure_find_configuration(
            &mut mock,
            device_plugin_service.config_name.clone(),
            device_plugin_service.config_namespace.clone(),
        );
        configure_find_instance(
            &mut mock,
            "../test/json/local-instance.json",
            device_plugin_service.instance_name.clone(),
            device_plugin_service.config_namespace.clone(),
            String::new(),
            NodeName::OtherNode,
            1,
        );
        let instance_name = device_plugin_service.instance_name.clone();
        let config_namespace = device_plugin_service.config_namespace.clone();
        mock.expect_update_instance()
            .times(1)
            .withf(move |instance, name, namespace| {
                namespace == config_namespace
                    && name == instance_name
                    && instance.nodes.contains(&"node-a".to_string())
            })
            .returning(move |_, _, _| Ok(()));

        let dps = Arc::new(device_plugin_service);
        if let DevicePluginBehavior::Instance(instance_device_plugin) = &dps.device_plugin_behavior
        {
            assert!(
                try_create_instance(dps.clone(), instance_device_plugin, Arc::new(mock))
                    .await
                    .is_ok()
            );
            assert!(dps
                .device_plugin_context
                .read()
                .await
                .instances
                .contains_key(&dps.instance_name));
        } else {
            panic!("Incorrect device plugin kind configured");
        }
    }

    // Test when instance already created and already contains this node.
    // Should find the instance but not update it.
    #[tokio::test]
    async fn test_try_create_instance_already_created_no_update() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (device_plugin_service, _device_plugin_service_receivers) =
            create_device_plugin_service(
                DevicePluginKind::Instance,
                InstanceConnectivityStatus::Online,
                false,
            );
        let mut mock = MockKubeInterface::new();
        configure_find_configuration(
            &mut mock,
            device_plugin_service.config_name.clone(),
            device_plugin_service.config_namespace.clone(),
        );
        configure_find_instance(
            &mut mock,
            "../test/json/local-instance.json",
            device_plugin_service.instance_name.clone(),
            device_plugin_service.config_namespace.clone(),
            String::new(),
            NodeName::ThisNode,
            1,
        );
        let dps = Arc::new(device_plugin_service);
        if let DevicePluginBehavior::Instance(instance_device_plugin) = &dps.device_plugin_behavior
        {
            assert!(
                try_create_instance(dps.clone(), instance_device_plugin, Arc::new(mock))
                    .await
                    .is_ok()
            );
            assert!(dps
                .device_plugin_context
                .read()
                .await
                .instances
                .contains_key(&dps.instance_name));
        } else {
            panic!("Incorrect device plugin kind configured");
        }
    }

    // Tests that try_create_instance returns error when trying to create an Instance for a Config that DNE
    #[tokio::test]
    async fn test_try_create_instance_no_config() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (device_plugin_service, _device_plugin_service_receivers) =
            create_device_plugin_service(
                DevicePluginKind::Instance,
                InstanceConnectivityStatus::Online,
                false,
            );
        let config_name = device_plugin_service.config_name.clone();
        let config_namespace = device_plugin_service.config_namespace.clone();
        let mut mock = MockKubeInterface::new();
        mock.expect_find_configuration()
            .times(1)
            .withf(move |name: &str, namespace: &str| {
                namespace == config_namespace && name == config_name
            })
            .returning(move |_, _| {
                let error = Error::new(ErrorKind::InvalidInput, "Configuration doesn't exist");
                Err(error.into())
            });
        let dps = Arc::new(device_plugin_service);
        if let DevicePluginBehavior::Instance(instance_device_plugin) = &dps.device_plugin_behavior
        {
            assert!(
                try_create_instance(dps.clone(), instance_device_plugin, Arc::new(mock))
                    .await
                    .is_err()
            );
        } else {
            panic!("Incorrect device plugin kind configured");
        }
    }

    // Tests that try_create_instance error
    #[tokio::test]
    async fn test_try_create_instance_error() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (device_plugin_service, _device_plugin_service_receivers) =
            create_device_plugin_service(
                DevicePluginKind::Instance,
                InstanceConnectivityStatus::Online,
                false,
            );
        let mut mock = MockKubeInterface::new();
        configure_find_configuration(
            &mut mock,
            device_plugin_service.config_name.clone(),
            device_plugin_service.config_namespace.clone(),
        );
        let instance_name = device_plugin_service.instance_name.clone();
        let config_name = device_plugin_service.config_name.clone();
        let config_uid = device_plugin_service.config_uid.clone();
        let config_namespace = device_plugin_service.config_namespace.clone();
        mock.expect_find_instance()
            .times(MAX_INSTANCE_UPDATE_TRIES as usize)
            .withf(move |name: &str, namespace: &str| {
                namespace == config_namespace && name == instance_name
            })
            .returning(move |_, _| Err(get_kube_not_found_error().into()));
        let instance_name = device_plugin_service.instance_name.clone();
        let config_namespace = device_plugin_service.config_namespace.clone();
        mock.expect_create_instance()
            .times(MAX_INSTANCE_UPDATE_TRIES as usize)
            .withf(move |instance, name, namespace, owner_name, owner_uid| {
                namespace == config_namespace
                    && name == instance_name
                    && instance.nodes.contains(&"node-a".to_string())
                    && owner_name == config_name
                    && owner_uid == config_uid
            })
            .returning(move |_, _, _, _, _| Err(anyhow::anyhow!("failure")));

        let dps = Arc::new(device_plugin_service);
        if let DevicePluginBehavior::Instance(instance_device_plugin) = &dps.device_plugin_behavior
        {
            assert!(
                try_create_instance(dps.clone(), instance_device_plugin, Arc::new(mock))
                    .await
                    .is_err()
            );
            assert!(!dps
                .device_plugin_context
                .read()
                .await
                .instances
                .contains_key(&dps.instance_name));
        } else {
            panic!("Incorrect device plugin kind configured");
        }
    }

    // Tests list_and_watch by creating DevicePluginService and DevicePlugin client (emulating kubelet)
    #[tokio::test]
    async fn test_internal_list_and_watch() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (device_plugin_service, _device_plugin_service_receivers) =
            create_device_plugin_service(
                DevicePluginKind::Instance,
                InstanceConnectivityStatus::Online,
                false,
            );
        let list_and_watch_message_sender =
            device_plugin_service.list_and_watch_message_sender.clone();
        let mut mock = MockKubeInterface::new();
        configure_find_configuration(
            &mut mock,
            device_plugin_service.config_name.clone(),
            device_plugin_service.config_namespace.clone(),
        );
        let instance_name = device_plugin_service.instance_name.clone();
        let config_name = device_plugin_service.config_name.clone();
        let config_uid = device_plugin_service.config_uid.clone();
        let config_namespace = device_plugin_service.config_namespace.clone();
        mock.expect_find_instance()
            .times(2)
            .withf(move |name: &str, namespace: &str| {
                namespace == config_namespace && name == instance_name
            })
            .returning(move |_, _| Err(get_kube_not_found_error().into()));
        let instance_name = device_plugin_service.instance_name.clone();
        let config_namespace = device_plugin_service.config_namespace.clone();
        mock.expect_create_instance()
            .withf(move |instance, name, namespace, owner_name, owner_uid| {
                namespace == config_namespace
                    && name == instance_name
                    && instance.nodes.contains(&"node-a".to_string())
                    && owner_name == config_name
                    && owner_uid == config_uid
            })
            .returning(move |_, _, _, _, _| Ok(()));

        let stream = device_plugin_service
            .internal_list_and_watch(Arc::new(mock))
            .await
            .unwrap()
            .into_inner();
        list_and_watch_message_sender
            .send(ListAndWatchMessageKind::End)
            .unwrap();
        if let Ok(list_and_watch_response) = stream.into_inner().recv().await.unwrap() {
            assert_eq!(
                list_and_watch_response.devices[0].id,
                format!("{}-0", device_plugin_service.instance_name)
            );
        };
    }

    fn setup_internal_allocate_tests(
        mock: &mut MockKubeInterface,
        device_plugin_service: &DevicePluginService,
        formerly_allocated_node: String,
        newly_allocated_node: Option<String>,
    ) -> Request<AllocateRequest> {
        let device_usage_id_slot = format!("{}-0", device_plugin_service.instance_name);
        let device_usage_id_slot_2 = device_usage_id_slot.clone();
        configure_find_instance(
            mock,
            "../test/json/local-instance.json",
            device_plugin_service.instance_name.clone(),
            device_plugin_service.config_namespace.clone(),
            formerly_allocated_node,
            NodeName::ThisNode,
            1,
        );
        if let Some(new_node) = newly_allocated_node {
            mock.expect_update_instance()
                .times(1)
                .withf(move |instance_to_update: &InstanceSpec, _, _| {
                    instance_to_update
                        .device_usage
                        .get(&device_usage_id_slot)
                        .unwrap()
                        == &new_node
                })
                .returning(move |_, _, _| Ok(()));
        }
        let devices_i_ds = vec![device_usage_id_slot_2];
        let container_requests = vec![v1beta1::ContainerAllocateRequest { devices_i_ds }];
        Request::new(AllocateRequest { container_requests })
    }

    // Test that environment variables set in a Configuration will be set in brokers
    #[tokio::test]
    async fn test_internal_allocate_env_vars() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (device_plugin_service, mut device_plugin_service_receivers) =
            create_device_plugin_service(
                DevicePluginKind::Instance,
                InstanceConnectivityStatus::Online,
                true,
            );
        let node_name = device_plugin_service.node_name.clone();
        let mut mock = MockKubeInterface::new();
        let request = setup_internal_allocate_tests(
            &mut mock,
            &device_plugin_service,
            String::new(),
            Some(node_name),
        );
        let broker_envs = device_plugin_service
            .internal_allocate(request, Arc::new(mock))
            .await
            .unwrap()
            .into_inner()
            .container_responses[0]
            .envs
            .clone();
        assert_eq!(broker_envs.get("RESOLUTION_WIDTH").unwrap(), "800");
        assert_eq!(broker_envs.get("RESOLUTION_HEIGHT").unwrap(), "600");
        // Check that Device properties are set as env vars by checking for
        // property of device created in `create_device_plugin_service`
        assert_eq!(
            broker_envs.get("DEVICE_LOCATION_INFO_B494B6").unwrap(),
            "endpoint"
        );
        assert!(device_plugin_service_receivers
            .instance_list_and_watch_message_receiver
            .try_recv()
            .is_err());
        assert_eq!(
            device_plugin_service_receivers
                .configuration_list_and_watch_message_receiver
                .recv()
                .await
                .unwrap(),
            ListAndWatchMessageKind::Continue
        );
    }

    // Test when device_usage[id] == ""
    // internal_allocate should set device_usage[id] = m.nodeName, return
    #[tokio::test]
    async fn test_internal_allocate_success() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (device_plugin_service, mut device_plugin_service_receivers) =
            create_device_plugin_service(
                DevicePluginKind::Instance,
                InstanceConnectivityStatus::Online,
                true,
            );
        let node_name = device_plugin_service.node_name.clone();
        let mut mock = MockKubeInterface::new();
        let request = setup_internal_allocate_tests(
            &mut mock,
            &device_plugin_service,
            String::new(),
            Some(node_name),
        );
        assert!(device_plugin_service
            .internal_allocate(request, Arc::new(mock),)
            .await
            .is_ok());
        assert!(device_plugin_service_receivers
            .instance_list_and_watch_message_receiver
            .try_recv()
            .is_err());
        assert_eq!(
            device_plugin_service_receivers
                .configuration_list_and_watch_message_receiver
                .recv()
                .await
                .unwrap(),
            ListAndWatchMessageKind::Continue
        );
    }

    // Test when device_usage[id] == self.nodeName
    // Expected behavior: internal_allocate should keep device_usage[id] == self.nodeName and
    // instance should not be updated
    #[tokio::test]
    async fn test_internal_allocate_deallocate() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (device_plugin_service, mut device_plugin_service_receivers) =
            create_device_plugin_service(
                DevicePluginKind::Instance,
                InstanceConnectivityStatus::Online,
                true,
            );
        let mut mock = MockKubeInterface::new();
        let request = setup_internal_allocate_tests(
            &mut mock,
            &device_plugin_service,
            "node-a".to_string(),
            None,
        );
        assert!(device_plugin_service
            .internal_allocate(request, Arc::new(mock))
            .await
            .is_ok());
        assert!(device_plugin_service_receivers
            .instance_list_and_watch_message_receiver
            .try_recv()
            .is_err());
        assert_eq!(
            device_plugin_service_receivers
                .configuration_list_and_watch_message_receiver
                .recv()
                .await
                .unwrap(),
            ListAndWatchMessageKind::Continue
        );
    }

    // Tests when device_usage[id] == <another node>
    // Expected behavior: should invoke list_and_watch, and return error
    #[tokio::test]
    async fn test_internal_allocate_taken() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (device_plugin_service, mut device_plugin_service_receivers) =
            create_device_plugin_service(
                DevicePluginKind::Instance,
                InstanceConnectivityStatus::Online,
                true,
            );
        let device_usage_id_slot = format!("{}-0", device_plugin_service.instance_name);
        let mut mock = MockKubeInterface::new();
        configure_find_instance(
            &mut mock,
            "../test/json/local-instance.json",
            device_plugin_service.instance_name.clone(),
            device_plugin_service.config_namespace.clone(),
            "other".to_string(),
            NodeName::ThisNode,
            1,
        );
        let devices_i_ds = vec![device_usage_id_slot];
        let container_requests = vec![v1beta1::ContainerAllocateRequest { devices_i_ds }];
        let requests = Request::new(AllocateRequest { container_requests });
        match device_plugin_service
            .internal_allocate(requests, Arc::new(mock))
            .await
        {
            Ok(_) => panic!(
                "internal allocate is expected to fail due to requested device already being used"
            ),
            Err(e) => assert_eq!(
                e.message(),
                "Requested device kind Instance already in use by other nodes"
            ),
        }
        assert_eq!(
            device_plugin_service_receivers
                .instance_list_and_watch_message_receiver
                .recv()
                .await
                .unwrap(),
            ListAndWatchMessageKind::Continue
        );
        assert!(device_plugin_service_receivers
            .configuration_list_and_watch_message_receiver
            .try_recv()
            .is_err());
    }

    // Tests when instance does not have the requested device usage id
    // Expected behavior: should invoke list_and_watch, and return error
    #[tokio::test]
    async fn test_internal_allocate_no_id() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (device_plugin_service, mut device_plugin_service_receivers) =
            create_device_plugin_service(
                DevicePluginKind::Instance,
                InstanceConnectivityStatus::Online,
                true,
            );
        let device_usage_id_slot = format!("{}-100", device_plugin_service.instance_name);
        let mut mock = MockKubeInterface::new();
        configure_find_instance(
            &mut mock,
            "../test/json/local-instance.json",
            device_plugin_service.instance_name.clone(),
            device_plugin_service.config_namespace.clone(),
            "other".to_string(),
            NodeName::ThisNode,
            1,
        );
        let devices_i_ds = vec![device_usage_id_slot];
        let container_requests = vec![v1beta1::ContainerAllocateRequest { devices_i_ds }];
        let requests = Request::new(AllocateRequest { container_requests });
        match device_plugin_service
            .internal_allocate(requests, Arc::new(mock))
            .await
        {
            Ok(_) => {
                panic!("internal allocate is expected to fail due to invalid device usage slot")
            }
            Err(e) => assert_eq!(e.message(), "Could not find device usage slot"),
        }
        assert_eq!(
            device_plugin_service_receivers
                .instance_list_and_watch_message_receiver
                .recv()
                .await
                .unwrap(),
            ListAndWatchMessageKind::Continue
        );
        assert!(device_plugin_service_receivers
            .configuration_list_and_watch_message_receiver
            .try_recv()
            .is_err());
    }

    // Tests correct device usage is returned when an Instance is found
    // Expected behavior: should return correct device usage state for all usage slots
    #[tokio::test]
    async fn test_get_instance_device_usage_state() {
        let _ = env_logger::builder().is_test(true).try_init();
        let node_name = "node-a";
        let instance_name = "instance-1";
        let instance_namespace = "test-namespace";
        let mock_device_usages = vec![(DeviceUsageKind::Free, "".to_string()); 5];
        let capacity = mock_device_usages.len() as i32;
        let mut kube_instance_builder = KubeInstanceBuilder::new(instance_name, instance_namespace);
        kube_instance_builder.add_node(node_name);
        kube_instance_builder.add_device_usages(instance_name, mock_device_usages);
        let kube_instance = kube_instance_builder.build();
        let mock_instances = vec![(instance_name.to_string(), kube_instance)];
        let mut mock = MockKubeInterface::new();
        setup_find_instance_with_mock_instances(&mut mock, instance_namespace, mock_instances);

        let device_usage_states = get_instance_device_usage_states(
            node_name,
            instance_name,
            instance_namespace,
            &capacity,
            Arc::new(mock),
        )
        .await;
        assert!(device_usage_states
            .into_iter()
            .all(|(_, v)| { v == DeviceUsageStatus::Free }));
    }

    // Tests correct device usage is returned when an Instance is not found
    // Expected behavior: should return DeviceUsageStatus::Unknown for all usage slots
    #[tokio::test]
    async fn test_get_instance_device_usage_state_no_instance() {
        let _ = env_logger::builder().is_test(true).try_init();
        let node_name = "node-a";
        let instance_name = "instance-1";
        let instance_namespace = "test-namespace";
        let capacity = 5i32;
        let mut mock = MockKubeInterface::new();
        setup_find_instance_with_not_found_err(&mut mock, instance_name, instance_namespace);

        let device_usage_states = get_instance_device_usage_states(
            node_name,
            instance_name,
            instance_namespace,
            &capacity,
            Arc::new(mock),
        )
        .await;
        assert!(device_usage_states
            .into_iter()
            .all(|(_, v)| { v == DeviceUsageStatus::Unknown }));
    }

    fn create_configuration_device_plugin_service(
        connectivity_status: InstanceConnectivityStatus,
        add_to_instance_map: bool,
    ) -> (DevicePluginService, DevicePluginServiceReceivers) {
        let (dps, receivers) = create_device_plugin_service(
            DevicePluginKind::Configuration,
            connectivity_status,
            add_to_instance_map,
        );

        (dps, receivers)
    }

    // Tests 0 virtual device id is returned if instance not found in InstanceConfig
    #[tokio::test]
    async fn test_get_available_virtual_devices_no_instance_in_instance_map() {
        let _ = env_logger::builder().is_test(true).try_init();
        let this_node = "node-a";
        let instance_namespace = "test-namespace";
        let capacity = 5;
        let device_plugin_context = DevicePluginContextBuilder::new().build();
        let mock = MockKubeInterface::new();

        let result = get_available_virtual_devices(
            &device_plugin_context.instances,
            this_node,
            instance_namespace,
            &capacity,
            Arc::new(mock),
        )
        .await;
        assert!(result.is_empty());
    }

    // Tests 0 virtual device id is returned if instance not found from kube find_instance
    #[tokio::test]
    async fn test_get_available_virtual_devices_no_kube_instance() {
        let _ = env_logger::builder().is_test(true).try_init();
        let this_node = "node-a";
        let instance_name = "instance-1";
        let instance_namespace = "test-namespace";
        let capacity = 5;
        let mut device_plugin_context_builder = DevicePluginContextBuilder::new();
        device_plugin_context_builder
            .add_instance(instance_name, &InstanceConnectivityStatus::Online);
        let device_plugin_context = device_plugin_context_builder.build();
        let mut mock = MockKubeInterface::new();
        setup_find_instance_with_not_found_err(&mut mock, instance_name, instance_namespace);

        let result = get_available_virtual_devices(
            &device_plugin_context.instances,
            this_node,
            instance_namespace,
            &capacity,
            Arc::new(mock),
        )
        .await;
        assert!(result.is_empty());
    }

    // Tests 0 virtual device id is returned if all slots are taken by other node
    #[tokio::test]
    async fn test_get_available_virtual_devices_all_taken_by_other_node() {
        let this_node = "node-a";
        let other_node = "other";
        let mock_device_usages = vec![(DeviceUsageKind::Instance, other_node.to_string()); 5];

        let result = run_get_available_virtual_devices_test(this_node, mock_device_usages).await;
        assert!(result.is_empty());
    }

    // Tests 0 virtual device id is returned if all slots are taken by Instance on the same node
    #[tokio::test]
    async fn test_get_available_virtual_devices_all_taken_by_instance() {
        let this_node = "node-a";
        let mock_device_usages = vec![(DeviceUsageKind::Instance, this_node.to_string()); 5];

        let result = run_get_available_virtual_devices_test(this_node, mock_device_usages).await;
        assert!(result.is_empty());
    }

    // Tests 1 virtual device id is returned if all slots are free
    #[tokio::test]
    async fn test_get_available_virtual_devices_all_free() {
        let this_node = "node-a";
        let expected_length = 1;
        let mock_device_usages = vec![(DeviceUsageKind::Free, "".to_string()); 5];

        let result = run_get_available_virtual_devices_test(this_node, mock_device_usages).await;
        assert_eq!(result.len(), expected_length);
    }

    // Tests 1 virtual device id is returned if all slots are taken by Configuration
    // with the same vdev_id
    #[tokio::test]
    async fn test_get_available_virtual_devices_all_taken_by_configuration_same_vdev_id() {
        let this_node = "node-a";
        let vdev_ids = ["vdev-a"; 5];
        let expected_length = 1;
        let mock_device_usages = vdev_ids
            .iter()
            .map(|id| {
                (
                    DeviceUsageKind::Configuration(id.to_string()),
                    this_node.to_string(),
                )
            })
            .collect::<Vec<_>>();

        let result = run_get_available_virtual_devices_test(this_node, mock_device_usages).await;
        assert_eq!(result.len(), expected_length);
    }

    // Tests correct virtual device ids are returned if all slots are taken by Configuration
    // with different vdev_id
    #[tokio::test]
    async fn test_get_available_virtual_devices_all_taken_by_configuration() {
        let this_node = "node-a";
        let vdev_ids = ["vdev-a", "vdev-b", "vdev-c", "vdev-d", "vdev-e"];
        let expected_length = vdev_ids.len();
        let mock_device_usages = vdev_ids
            .iter()
            .map(|id| {
                (
                    DeviceUsageKind::Configuration(id.to_string()),
                    this_node.to_string(),
                )
            })
            .collect::<Vec<_>>();

        let result = run_get_available_virtual_devices_test(this_node, mock_device_usages).await;
        assert_eq!(result.len(), expected_length);
    }

    // Tests correct virtual device ids are returned if some slots are taken by Configuration
    // with different vdev_id
    #[tokio::test]
    async fn test_get_available_virtual_devices_some_taken_by_configuration() {
        let this_node = "node-a";
        let vdev_ids = ["vdev-a", "vdev-b"];
        let expected_length = 2 + 1; // 2 vdev_ids + 1 free
        let mut mock_device_usages = vec![(DeviceUsageKind::Free, "".to_string()); 3];
        mock_device_usages.extend(
            vdev_ids
                .iter()
                .map(|id| {
                    (
                        DeviceUsageKind::Configuration(id.to_string()),
                        this_node.to_string(),
                    )
                })
                .collect::<Vec<_>>(),
        );

        let result = run_get_available_virtual_devices_test(this_node, mock_device_usages).await;
        assert_eq!(result.len(), expected_length);
    }

    async fn run_get_available_virtual_devices_test(
        node_name: &str,
        device_usages: Vec<(DeviceUsageKind, String)>,
    ) -> HashSet<String> {
        let _ = env_logger::builder().is_test(true).try_init();
        let instance_name = "instance-1";
        let instance_namespace = "test-namespace";
        let capacity = device_usages.len() as i32;

        let mut device_plugin_context_builder = DevicePluginContextBuilder::new();
        device_plugin_context_builder
            .add_instance(instance_name, &InstanceConnectivityStatus::Online);
        let device_plugin_context = device_plugin_context_builder.build();

        let mut kube_instance_builder = KubeInstanceBuilder::new(instance_name, instance_namespace);
        kube_instance_builder.add_node(node_name);
        kube_instance_builder.add_device_usages(instance_name, device_usages);
        let kube_instance = kube_instance_builder.build();
        let mock_instances = vec![(instance_name.to_string(), kube_instance)];
        let mut mock = MockKubeInterface::new();
        setup_find_instance_with_mock_instances(&mut mock, instance_namespace, mock_instances);

        get_available_virtual_devices(
            &device_plugin_context.instances,
            node_name,
            instance_namespace,
            &capacity,
            Arc::new(mock),
        )
        .await
    }

    #[derive(Default, Clone)]
    struct DevicePluginContextBuilder {
        device_plugin_context: DevicePluginContext,
    }

    impl DevicePluginContextBuilder {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn add_instance(
            &mut self,
            instance_name: &str,
            connectivity_status: &InstanceConnectivityStatus,
        ) -> &mut Self {
            let (list_and_watch_message_sender, _) = broadcast::channel(4);
            let instance_info = InstanceInfo {
                list_and_watch_message_sender,
                connectivity_status: connectivity_status.clone(),
                instance_id: format!("{}-instance-id", instance_name),
                device: Device {
                    id: format!("{}-device-id", instance_name),
                    properties: HashMap::new(),
                    mounts: Vec::new(),
                    device_specs: Vec::new(),
                },
            };
            self.device_plugin_context
                .instances
                .insert(instance_name.to_string(), instance_info);
            self
        }

        pub fn build(&self) -> DevicePluginContext {
            self.device_plugin_context.clone()
        }
    }

    #[derive(Clone)]
    struct KubeInstanceBuilder {
        name: String,
        namespace: String,
        configuration_name: String,
        nodes: Vec<String>,
        shared: bool,
        device_usages: HashMap<String, Vec<(DeviceUsageKind, String)>>,
    }

    impl KubeInstanceBuilder {
        pub fn new(name: &str, namespace: &str) -> Self {
            Self {
                name: name.to_string(),
                namespace: namespace.to_string(),
                configuration_name: String::default(),
                nodes: Vec::new(),
                shared: true,
                device_usages: HashMap::new(),
            }
        }

        pub fn add_node(&mut self, node: &str) -> &mut Self {
            self.nodes.push(node.to_string());
            self
        }

        pub fn add_device_usage(
            &mut self,
            instance_name: &str,
            device_usage: (DeviceUsageKind, String),
        ) -> &mut Self {
            self.device_usages
                .entry(instance_name.to_string())
                .or_default()
                .push(device_usage);
            self
        }

        pub fn add_device_usages(
            &mut self,
            instance_name: &str,
            device_usages: Vec<(DeviceUsageKind, String)>,
        ) -> &mut Self {
            self.device_usages
                .entry(instance_name.to_string())
                .or_default()
                .extend(device_usages);
            self
        }

        pub fn build(&self) -> Instance {
            let instance_json = format!(
                r#"{{
                "apiVersion": "akri.sh/v0",
                "kind": "Instance",
                "metadata": {{
                    "name": "{}",
                    "namespace": "{}",
                    "uid": "abcdegfh-ijkl-mnop-qrst-uvwxyz012345"
                }},
                "spec": {{
                    "configurationName": "",
                    "nodes": [],
                    "shared": true,
                    "deviceUsage": {{
                    }}
                }}
            }}
            "#,
                self.name, self.namespace
            );
            let mut instance: Instance = serde_json::from_str(&instance_json).unwrap();
            instance.spec.configuration_name = self.configuration_name.clone();
            instance.spec.nodes = self.nodes.clone();
            instance.spec.shared = self.shared;
            instance.spec.device_usage = self
                .device_usages
                .iter()
                .flat_map(|(instance_name, usages)| {
                    usages.iter().enumerate().map(move |(pos, (kind, node))| {
                        let key = format!("{}-{}", instance_name, pos);
                        (key, NodeUsage::create(kind, node).unwrap().to_string())
                    })
                })
                .collect::<HashMap<_, _>>();
            instance
        }
    }

    // Tests correct virtual devices are returned if all usage slots are free
    #[tokio::test]
    async fn test_get_virtual_device_resources_all_free() {
        let mock_instance_data = HashMap::from([("instance-1", None), ("instance-2", None)]);
        let request_vdev_ids = vec!["vdev-a", "vdev-b"];

        let result =
            run_get_virtual_device_resources_test(mock_instance_data, request_vdev_ids.clone())
                .await;
        assert_eq!(result.unwrap().len(), request_vdev_ids.len());
    }

    // Tests correct virtual devices are returned if all usage slots taken by Configuration(same vdev id)
    #[tokio::test]
    async fn test_get_virtual_device_resources_all_taken_by_configuration_same_vdev_id() {
        let mock_instance_data = HashMap::from([
            ("instance-1", Some("vdev-a")),
            ("instance-2", Some("vdev-b")),
        ]);
        let request_vdev_ids = vec!["vdev-a", "vdev-b"];

        let result =
            run_get_virtual_device_resources_test(mock_instance_data, request_vdev_ids.clone())
                .await;
        assert_eq!(result.unwrap().len(), request_vdev_ids.len());
    }

    // Tests correct virtual devices are returned if all usage slots are free or taken by Configuration(same vdev id)
    #[tokio::test]
    async fn test_get_virtual_device_resources_free_or_taken_by_configuration_same_vdev_id() {
        let mock_instance_data =
            HashMap::from([("instance-1", None), ("instance-2", Some("vdev-b"))]);
        let request_vdev_ids = vec!["vdev-a", "vdev-b"];

        let result =
            run_get_virtual_device_resources_test(mock_instance_data, request_vdev_ids.clone())
                .await;
        assert_eq!(result.unwrap().len(), request_vdev_ids.len());
    }

    // Tests get_virtual_device_resources returns err if all usage slots taken by Configuration(different vdev id)
    #[tokio::test]
    async fn test_get_virtual_device_resources_all_taken_by_configuration_different_vdev_id() {
        let mock_instance_data = HashMap::from([
            ("instance-1", Some("other-vdev-a")),
            ("instance-2", Some("other-vdev-b")),
        ]);
        let request_vdev_ids = vec!["vdev-a", "vdev-b"];

        let result =
            run_get_virtual_device_resources_test(mock_instance_data, request_vdev_ids).await;
        assert!(result.is_err());
    }

    // Tests get_virtual_device_resources returns err if one instance usage slots taken by Configuration(different vdev id)
    #[tokio::test]
    async fn test_get_virtual_device_resources_some_taken_by_configuration_different_vdev_id() {
        let mock_instance_data = HashMap::from([
            ("instance-1", Some("other-vdev-a")),
            ("instance-2", Some("vdev-b")),
        ]);
        let request_vdev_ids = vec!["vdev-1", "vdev-2"];

        let result =
            run_get_virtual_device_resources_test(mock_instance_data, request_vdev_ids).await;
        assert!(result.is_err());
    }

    // Tests get_virtual_device_resources returns err if one instance usage slots taken by Configuration(different vdev id)
    #[tokio::test]
    async fn test_get_virtual_device_resources_free_or_some_taken_by_configuration_different_vdev_id(
    ) {
        let mock_instance_data =
            HashMap::from([("instance-1", Some("other-vdev-a")), ("instance-2", None)]);
        let request_vdev_ids = vec!["vdev-1", "vdev-2"];

        let result =
            run_get_virtual_device_resources_test(mock_instance_data, request_vdev_ids).await;
        assert!(result.is_err());
    }

    async fn run_get_virtual_device_resources_test(
        instance_data: HashMap<&str, Option<&str>>,
        request_vdev_ids: Vec<&str>,
    ) -> Result<HashMap<String, (String, String)>, Status> {
        let _ = env_logger::builder().is_test(true).try_init();
        let this_node = "node-a";
        let instance_namespace = "test-namespace";
        let capacity = 5;
        let mut device_plugin_context_builder = DevicePluginContextBuilder::new();
        instance_data.keys().for_each(|instance_name| {
            device_plugin_context_builder
                .add_instance(instance_name, &InstanceConnectivityStatus::Online);
        });
        let device_plugin_context = device_plugin_context_builder.build();
        let mock_instances = instance_data
            .iter()
            .map(|(instance_name, mock_vdev_id)| {
                let device_usage = if let Some(id) = mock_vdev_id {
                    (
                        DeviceUsageKind::Configuration(id.to_string()),
                        this_node.to_string(),
                    )
                } else {
                    (DeviceUsageKind::Free, "".to_string())
                };
                let mut kube_instance_builder =
                    KubeInstanceBuilder::new(instance_name, instance_namespace);
                kube_instance_builder.add_node(this_node);
                kube_instance_builder.add_device_usage(instance_name, device_usage);

                (instance_name.to_string(), kube_instance_builder.build())
            })
            .collect::<Vec<_>>();
        let mut mock = MockKubeInterface::new();
        setup_find_instance_with_mock_instances(&mut mock, instance_namespace, mock_instances);

        get_virtual_device_resources(
            request_vdev_ids.iter().map(|x| x.to_string()).collect(),
            &device_plugin_context.instances,
            this_node,
            instance_namespace,
            &capacity,
            Arc::new(mock),
        )
        .await
    }

    // Configuration resource from instance, no instance available, should receive nothing from the response stream
    #[tokio::test]
    async fn test_cdps_internal_list_and_watch_no_instance() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (device_plugin_service, _device_plugin_service_receivers) =
            create_configuration_device_plugin_service(InstanceConnectivityStatus::Online, false);
        let list_and_watch_message_sender =
            device_plugin_service.list_and_watch_message_sender.clone();
        let mock = MockKubeInterface::new();

        let stream = device_plugin_service
            .internal_list_and_watch(Arc::new(mock))
            .await
            .unwrap()
            .into_inner();
        list_and_watch_message_sender
            .send(ListAndWatchMessageKind::End)
            .unwrap();
        assert!(stream.into_inner().try_recv().is_err());
    }

    // Configuration resource from instance, instance available, should return capacity virtual devices
    #[tokio::test]
    async fn test_cdps_internal_list_and_watch_with_instance() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (device_plugin_service, _device_plugin_service_receivers) =
            create_configuration_device_plugin_service(InstanceConnectivityStatus::Online, true);
        let list_and_watch_message_sender =
            device_plugin_service.list_and_watch_message_sender.clone();
        let mut mock = MockKubeInterface::new();
        configure_find_instance(
            &mut mock,
            "../test/json/local-instance.json",
            device_plugin_service.instance_name.clone(),
            device_plugin_service.config_namespace.clone(),
            String::new(),
            NodeName::OtherNode,
            1,
        );
        let stream = device_plugin_service
            .internal_list_and_watch(Arc::new(mock))
            .await
            .unwrap()
            .into_inner();
        list_and_watch_message_sender
            .send(ListAndWatchMessageKind::End)
            .unwrap();

        let result = stream.into_inner().recv().await.unwrap();
        let list_and_watch_response = result.unwrap();
        assert_eq!(list_and_watch_response.devices.len(), 1);
    }

    fn setup_configuration_internal_allocate_tests(
        mock: &mut MockKubeInterface,
        request_device_id: &str,
        config_namespace: &str,
        instance_name: &str,
        formerly_device_usage: &NodeUsage,
        newly_device_usage: Option<&NodeUsage>,
        expected_calls: usize,
    ) -> Request<AllocateRequest> {
        let formerly_device_usage_string = formerly_device_usage.to_string();
        configure_find_instance(
            mock,
            "../test/json/local-instance.json",
            instance_name.to_string(),
            config_namespace.to_string(),
            formerly_device_usage_string,
            NodeName::ThisNode,
            expected_calls,
        );
        if let Some(new_device_usage) = newly_device_usage {
            let expected_device_usage_string = new_device_usage.to_string();
            mock.expect_update_instance()
                .times(1)
                .withf(move |instance_to_update: &InstanceSpec, _, _| {
                    let usages = instance_to_update
                        .device_usage
                        .iter()
                        .filter(|(_, usage)| *usage == &expected_device_usage_string)
                        .collect::<Vec<_>>();
                    usages.len() == 1
                })
                .returning(move |_, _, _| Ok(()));
        }
        let devices_i_ds = vec![request_device_id.to_string()];
        let container_requests = vec![v1beta1::ContainerAllocateRequest { devices_i_ds }];
        Request::new(AllocateRequest { container_requests })
    }

    // Test that environment variables set in a Configuration will be set in brokers
    #[tokio::test]
    async fn test_cdps_internal_allocate_env_vars() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (device_plugin_service, mut device_plugin_service_receivers) =
            create_configuration_device_plugin_service(InstanceConnectivityStatus::Online, true);
        let node_name = device_plugin_service.node_name.clone();
        let instance_name = device_plugin_service
            .device_plugin_context
            .read()
            .await
            .instances
            .keys()
            .next()
            .unwrap()
            .to_string();
        let request_vdev_id = "123456";
        let mut mock = MockKubeInterface::new();
        let request = setup_configuration_internal_allocate_tests(
            &mut mock,
            request_vdev_id,
            &device_plugin_service.config_namespace,
            &instance_name,
            &NodeUsage::default(),
            Some(
                &NodeUsage::create(
                    &DeviceUsageKind::Configuration(request_vdev_id.to_string()),
                    &node_name,
                )
                .unwrap(),
            ),
            2,
        );
        let broker_envs = device_plugin_service
            .internal_allocate(request, Arc::new(mock))
            .await
            .unwrap()
            .into_inner()
            .container_responses[0]
            .envs
            .clone();
        assert_eq!(broker_envs.get("RESOLUTION_WIDTH").unwrap(), "800");
        assert_eq!(broker_envs.get("RESOLUTION_HEIGHT").unwrap(), "600");
        // Check that Device properties are set as env vars by checking for
        // property of device created in `create_configuration_device_plugin_service`
        assert_eq!(
            broker_envs.get("DEVICE_LOCATION_INFO_B494B6").unwrap(),
            "endpoint"
        );
        assert!(device_plugin_service_receivers
            .configuration_list_and_watch_message_receiver
            .try_recv()
            .is_err());
        assert_eq!(
            device_plugin_service_receivers
                .instance_list_and_watch_message_receiver
                .recv()
                .await
                .unwrap(),
            ListAndWatchMessageKind::Continue
        );
    }

    // Test when device_usage[id] == ""
    // internal_allocate should set device_usage[id] = C:vdev_id:nodeName, return
    #[tokio::test]
    async fn test_cdps_internal_allocate_success() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (device_plugin_service, mut device_plugin_service_receivers) =
            create_configuration_device_plugin_service(InstanceConnectivityStatus::Online, true);
        let node_name = device_plugin_service.node_name.clone();
        let instance_name = device_plugin_service
            .device_plugin_context
            .read()
            .await
            .instances
            .keys()
            .next()
            .unwrap()
            .to_string();
        let request_vdev_id = "123456";
        let mut mock = MockKubeInterface::new();
        let request = setup_configuration_internal_allocate_tests(
            &mut mock,
            request_vdev_id,
            &device_plugin_service.config_namespace,
            &instance_name,
            &NodeUsage::default(),
            Some(
                &NodeUsage::create(
                    &DeviceUsageKind::Configuration(request_vdev_id.to_string()),
                    &node_name,
                )
                .unwrap(),
            ),
            2,
        );
        assert!(device_plugin_service
            .internal_allocate(request, Arc::new(mock))
            .await
            .is_ok());
        assert!(device_plugin_service_receivers
            .configuration_list_and_watch_message_receiver
            .try_recv()
            .is_err());
        assert_eq!(
            device_plugin_service_receivers
                .instance_list_and_watch_message_receiver
                .recv()
                .await
                .unwrap(),
            ListAndWatchMessageKind::Continue
        );
    }

    // Test when device_usage[id] == C:vdev_id:nodeName
    // Expected behavior: internal_allocate should return success, this can happen when
    // a device is allocated for a work load, after the work load finishs and exits
    // the device plugin framework will re-allocate the device for other work loads to use
    #[tokio::test]
    async fn test_cdps_internal_reallocate() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (device_plugin_service, mut device_plugin_service_receivers) =
            create_configuration_device_plugin_service(InstanceConnectivityStatus::Online, true);
        let node_name = device_plugin_service.node_name.clone();
        let instance_name = device_plugin_service
            .device_plugin_context
            .read()
            .await
            .instances
            .keys()
            .next()
            .unwrap()
            .to_string();
        let request_vdev_id = "123456";
        let mut mock = MockKubeInterface::new();
        let request = setup_configuration_internal_allocate_tests(
            &mut mock,
            request_vdev_id,
            &device_plugin_service.config_namespace,
            &instance_name,
            &NodeUsage::create(
                &DeviceUsageKind::Configuration(request_vdev_id.to_string()),
                &node_name,
            )
            .unwrap(),
            None,
            2,
        );
        assert!(device_plugin_service
            .internal_allocate(request, Arc::new(mock))
            .await
            .is_ok());
        assert!(device_plugin_service_receivers
            .configuration_list_and_watch_message_receiver
            .try_recv()
            .is_err());
    }

    // Tests when device_usage[id] == <another node>
    // Expected behavior: should invoke list_and_watch, and return error
    #[tokio::test]
    async fn test_cdps_internal_allocate_taken_by_other_node() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (device_plugin_service, mut device_plugin_service_receivers) =
            create_configuration_device_plugin_service(InstanceConnectivityStatus::Online, true);
        let instance_name = device_plugin_service
            .device_plugin_context
            .read()
            .await
            .instances
            .keys()
            .next()
            .unwrap()
            .to_string();
        let request_vdev_id = "123456";
        let mut mock = MockKubeInterface::new();
        let request = setup_configuration_internal_allocate_tests(
            &mut mock,
            request_vdev_id,
            &device_plugin_service.config_namespace,
            &instance_name,
            &NodeUsage::create(
                &DeviceUsageKind::Configuration(request_vdev_id.to_string()),
                "other",
            )
            .unwrap(),
            None,
            1,
        );
        assert!(device_plugin_service
            .internal_allocate(request, Arc::new(mock))
            .await
            .is_err());
        assert_eq!(
            device_plugin_service_receivers
                .configuration_list_and_watch_message_receiver
                .recv()
                .await
                .unwrap(),
            ListAndWatchMessageKind::Continue
        );
    }

    // Tests when device_usage[id] == <another instance>
    // Expected behavior: should invoke list_and_watch, and return error
    #[tokio::test]
    async fn test_cdps_internal_allocate_taken_by_instance() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (device_plugin_service, mut device_plugin_service_receivers) =
            create_configuration_device_plugin_service(InstanceConnectivityStatus::Online, true);
        let node_name = device_plugin_service.node_name.clone();
        let instance_name = device_plugin_service
            .device_plugin_context
            .read()
            .await
            .instances
            .keys()
            .next()
            .unwrap()
            .to_string();
        let request_vdev_id = "123456";
        let mut mock = MockKubeInterface::new();
        let request = setup_configuration_internal_allocate_tests(
            &mut mock,
            request_vdev_id,
            &device_plugin_service.config_namespace,
            &instance_name,
            &NodeUsage::create(&DeviceUsageKind::Instance, &node_name).unwrap(),
            None,
            1,
        );
        assert!(device_plugin_service
            .internal_allocate(request, Arc::new(mock))
            .await
            .is_err());
        assert_eq!(
            device_plugin_service_receivers
                .configuration_list_and_watch_message_receiver
                .recv()
                .await
                .unwrap(),
            ListAndWatchMessageKind::Continue
        );
    }
}
