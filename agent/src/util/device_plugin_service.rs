use super::constants::{
    HEALTHY, K8S_DEVICE_PLUGIN_VERSION, KUBELET_SOCKET, LIST_AND_WATCH_SLEEP_SECS, UNHEALTHY,
};
use super::v1beta1;
use super::v1beta1::{
    device_plugin_server::{DevicePlugin, DevicePluginServer},
    registration_client, AllocateRequest, AllocateResponse, DevicePluginOptions, Empty,
    ListAndWatchResponse, PreStartContainerRequest, PreStartContainerResponse,
};
use akri_shared::{
    akri::{
        configuration::{Configuration, ProtocolHandler},
        instance::Instance,
        retry::{random_delay, MAX_INSTANCE_UPDATE_TRIES},
        AKRI_PREFIX, AKRI_SLOT_ANNOTATION_NAME,
    },
    k8s,
    k8s::KubeInterface,
};
use futures::stream::TryStreamExt;
use log::{error, info, trace};
use std::{
    collections::HashMap,
    convert::TryFrom,
    env,
    path::Path,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    net::UnixListener,
    net::UnixStream,
    sync::{broadcast, mpsc, Mutex},
    task,
    time::{delay_for, timeout},
};
use tonic::{
    transport::{Endpoint, Server, Uri},
    Code, Request, Response, Status,
};
use tower::service_fn;

/// Message sent in channel to `list_and_watch`.
/// Dictates what action `list_and_watch` should take upon being awoken.
#[derive(PartialEq, Clone, Debug)]
pub enum ListAndWatchMessageKind {
    /// Prematurely continue looping
    Continue,
    /// Stop looping
    End,
}

/// Describes the discoverability of an instance for this node
#[derive(PartialEq, Debug, Clone)]
pub enum ConnectivityStatus {
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
    /// Instance's `ConnectivityStatus`
    pub connectivity_status: ConnectivityStatus,
}

pub type InstanceMap = Arc<Mutex<HashMap<String, InstanceInfo>>>;

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
    instance_name: String,
    /// Socket endpoint
    endpoint: String,
    /// Instance's Configuration
    config: Configuration,
    /// Name of Instance's Configuration CRD
    config_name: String,
    /// UID of Instance's Configuration CRD
    config_uid: String,
    /// Namespace of Instance's Configuration CRD
    config_namespace: String,
    /// Instance is [not]shared
    shared: bool,
    /// Hostname of node this Device Plugin is running on
    node_name: String,
    /// Information that must be communicated with broker. Stored in Instance CRD as metadata.
    instance_properties: HashMap<String, String>,
    /// Map of all Instances that have the same Configuration CRD as this one
    instance_map: InstanceMap,
    /// Receiver for list_and_watch continue or end messages
    /// Note: since the tonic grpc generated list_and_watch definition takes in &self,
    /// using broadcast sender instead of mpsc receiver
    /// Can clone broadcast sender and subscribe receiver to use in spawned thread in list_and_watch
    list_and_watch_message_sender: broadcast::Sender<ListAndWatchMessageKind>,
    /// Upon send, terminates function that acts as the shutdown signal for this service
    server_ender_sender: mpsc::Sender<()>,
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
            pre_start_required: true,
        };
        Ok(Response::new(resp))
    }

    type ListAndWatchStream = mpsc::Receiver<Result<ListAndWatchResponse, Status>>;

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
        let dps = Arc::new(self.clone());
        let mut list_and_watch_message_receiver = self.list_and_watch_message_sender.subscribe();

        // Create a channel that list_and_watch can periodically send updates to kubelet on
        let (mut kubelet_update_sender, kubelet_update_receiver) = mpsc::channel(4);
        // Spawn thread so can send kubelet the receiving end of the channel to listen on
        tokio::spawn(async move {
            let mut keep_looping = true;
            #[cfg(not(test))]
            let kube_interface = Arc::new(k8s::create_kube_interface());

            // Try to create an Instance CRD for this plugin and add it to the global InstanceMap else shutdown
            #[cfg(not(test))]
            {
                if let Err(e) = try_create_instance(dps.clone(), kube_interface.clone()).await {
                    error!(
                        "list_and_watch - ending service because could not create instance {} with error {}",
                        dps.instance_name,
                        e
                    );
                    dps.server_ender_sender.clone().send(()).await.unwrap();
                    keep_looping = false;
                }
            }

            while keep_looping {
                trace!(
                    "list_and_watch - loop iteration for Instance {}",
                    dps.instance_name
                );

                let virtual_devices: Vec<v1beta1::Device>;
                #[cfg(test)]
                {
                    virtual_devices =
                        build_unhealthy_virtual_devices(dps.config.capacity, &dps.instance_name);
                }
                #[cfg(not(test))]
                {
                    virtual_devices =
                        build_list_and_watch_response(dps.clone(), kube_interface.clone())
                            .await
                            .unwrap();
                }

                let resp = v1beta1::ListAndWatchResponse {
                    devices: virtual_devices,
                };

                // Send virtual devices list back to kubelet
                if let Err(e) = kubelet_update_sender.send(Ok(resp)).await {
                    trace!(
                        "list_and_watch - for Instance {} kubelet no longer receiving with error {}",
                        dps.instance_name,
                        e
                    );
                    // This means kubelet is down/has been restarted. Remove instance from instance map so
                    // do_periodic_discovery will create a new device plugin service for this instance.
                    dps.instance_map.lock().await.remove(&dps.instance_name);
                    dps.server_ender_sender.clone().send(()).await.unwrap();
                    keep_looping = false;
                }
                // Sleep for LIST_AND_WATCH_SLEEP_SECS unless receive message to shutdown the server
                // or continue (and send another list of devices)
                match timeout(
                    Duration::from_secs(LIST_AND_WATCH_SLEEP_SECS),
                    list_and_watch_message_receiver.recv(),
                )
                .await
                {
                    Ok(message) => {
                        // If receive message to end list_and_watch, send list of unhealthy devices
                        // and shutdown the server by sending message on server_ender_sender channel
                        if message == Ok(ListAndWatchMessageKind::End) {
                            trace!(
                                "list_and_watch - for Instance {} received message to end",
                                dps.instance_name
                            );
                            let devices = build_unhealthy_virtual_devices(
                                dps.config.capacity,
                                &dps.instance_name,
                            );
                            kubelet_update_sender.send(Ok(v1beta1::ListAndWatchResponse { devices }))
                                .await
                                .unwrap();
                            dps.server_ender_sender.clone().send(()).await.unwrap();
                            keep_looping = false;
                        }
                    }
                    Err(_) => trace!(
                        "list_and_watch - for Instance {} did not receive a message for {} seconds ... continuing", dps.instance_name, LIST_AND_WATCH_SLEEP_SECS
                    ),
                }
            }
            trace!("list_and_watch - for Instance {} ending", dps.instance_name);
        });
        Ok(Response::new(kubelet_update_receiver))
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
        let kube_interface = Arc::new(k8s::create_kube_interface());
        match self.internal_allocate(requests, kube_interface).await {
            Ok(resp) => Ok(resp),
            Err(e) => Err(e),
        }
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
    /// Called when kubelet is trying to reserve for this node a usage slot (or virtual device) of the Instance.
    /// Tries to update Instance CRD to reserve the requested slot. If cannot reserve that slot, forces `list_and_watch` to continue
    /// (sending kubelet the latest list of slots) and returns error, so kubelet will not schedule the pod to this node.
    async fn internal_allocate(
        &self,
        requests: Request<AllocateRequest>,
        kube_interface: Arc<impl KubeInterface>,
    ) -> Result<Response<AllocateResponse>, Status> {
        let mut container_responses: Vec<v1beta1::ContainerAllocateResponse> = Vec::new();

        for request in requests.into_inner().container_requests {
            trace!(
                "internal_allocate - for Instance {} handling request {:?}",
                &self.instance_name,
                request,
            );
            let mut akri_annotations = std::collections::HashMap::new();
            for device_usage_id in request.devices_i_ds {
                trace!(
                    "internal_allocate - for Instance {} processing request for device usage slot id {}",
                    &self.instance_name,
                    device_usage_id
                );

                akri_annotations.insert(
                    AKRI_SLOT_ANNOTATION_NAME.to_string(),
                    device_usage_id.clone(),
                );

                if let Err(e) = try_update_instance_device_usage(
                    &device_usage_id,
                    &self.node_name,
                    &self.instance_name,
                    &self.config_namespace,
                    kube_interface.clone(),
                )
                .await
                {
                    trace!("internal_allocate - could not assign {} slot to {} node ... forcing list_and_watch to continue", device_usage_id, &self.node_name);
                    self.list_and_watch_message_sender
                        .send(ListAndWatchMessageKind::Continue)
                        .unwrap();
                    return Err(e);
                }

                trace!(
                    "internal_allocate - finished processing device_usage_id {}",
                    device_usage_id
                );
            }
            // Successfully reserved device_usage_slot[s] for this node.
            // Add response to list of responses
            let response = build_container_allocate_response(
                akri_annotations,
                &self.instance_properties,
                &self.config.protocol,
            );
            container_responses.push(response);
        }
        trace!(
            "internal_allocate - for Instance {} returning responses",
            &self.instance_name
        );
        Ok(Response::new(v1beta1::AllocateResponse {
            container_responses,
        }))
    }
}

/// This returns the value that should be inserted at `device_usage_id` slot for an instance else an error.
/// # More details
/// Cases based on the usage slot (`device_usage_id`) value
/// 1. device_usage[id] == "" ... this means that the device is available for use
///     * <ACTION> return this node name
/// 2. device_usage[id] == self.nodeName ... this means THIS node previously used id, but the DevicePluginManager knows that this is no longer true
///     * <ACTION> return ""
/// 3. device_usage[id] == <some other node> ... this means that we believe this device is in use by another node and should be marked unhealthy
///     * <ACTION> return error
/// 4. No corresponding id found ... this is an unknown error condition (BAD)
///     * <ACTION> return error
fn get_slot_value(
    device_usage_id: &str,
    node_name: &str,
    instance: &Instance,
) -> Result<String, Status> {
    if let Some(allocated_node) = instance.device_usage.get(device_usage_id) {
        if allocated_node == "" {
            Ok(node_name.to_string())
        } else if allocated_node == node_name {
            Ok("".to_string())
        } else {
            trace!("internal_allocate - request for device slot {} previously claimed by a diff node {} than this one {} ... indicates the device on THIS node must be marked unhealthy, invoking ListAndWatch ... returning failure, next scheduling should succeed!", device_usage_id, allocated_node, node_name);
            Err(Status::new(
                Code::Unknown,
                "Requested device already in use",
            ))
        }
    } else {
        // No corresponding id found
        trace!(
            "internal_allocate - could not find {} id in device_usage",
            device_usage_id
        );
        Err(Status::new(
            Code::Unknown,
            "Could not find device usage slot",
        ))
    }
}

/// This tries up to `MAX_INSTANCE_UPDATE_TRIES` to update the requested slot of the Instance with the appropriate value (either "" to clear slot or node_name).
/// It cannot be assumed that this will successfully update Instance on first try since Device Plugins on other nodes may be simultaneously trying to update the Instance.
/// This returns an error if slot does not need to be updated or `MAX_INSTANCE_UPDATE_TRIES` attempted.
async fn try_update_instance_device_usage(
    device_usage_id: &str,
    node_name: &str,
    instance_name: &str,
    instance_namespace: &str,
    kube_interface: Arc<impl KubeInterface>,
) -> Result<(), Status> {
    let mut instance: Instance;
    for x in 0..MAX_INSTANCE_UPDATE_TRIES {
        // Grab latest instance
        match kube_interface
            .find_instance(&instance_name, &instance_namespace)
            .await
        {
            Ok(instance_object) => instance = instance_object.spec,
            Err(_) => {
                trace!(
                    "internal_allocate - could not find Instance {}",
                    instance_name
                );
                return Err(Status::new(
                    Code::Unknown,
                    format!("Could not find Instance {}", instance_name),
                ));
            }
        }

        // at this point, `value` should either be:
        //   * `node_name`: meaning that this node is claiming this slot
        //   * "": meaning this node previously claimed this slot, but kubelet
        //          knows that claim is no longer valid.  In this case, reset the
        //          slot (which triggers each node to set the slot as Healthy) to
        //          allow a fair rescheduling of the workload
        let value = get_slot_value(device_usage_id, node_name, &instance)?;
        instance
            .device_usage
            .insert(device_usage_id.to_string(), value.clone());

        match kube_interface
            .update_instance(&instance, &instance_name, &instance_namespace)
            .await
        {
            Ok(()) => {
                if value == node_name {
                    return Ok(());
                } else {
                    return Err(Status::new(Code::Unknown, "Devices are in inconsistent state, updated device usage, please retry scheduling"));
                }
            }
            Err(e) => {
                if x == (MAX_INSTANCE_UPDATE_TRIES - 1) {
                    trace!("internal_allocate - update_instance returned error [{}] after max tries ... returning error", e);
                    return Err(Status::new(Code::Unknown, "Could not update Instance"));
                }
            }
        }
        random_delay().await;
    }
    Ok(())
}

/// This sets the volume mounts and environment variables according to the instance's protocol.
fn build_container_allocate_response(
    annotations: HashMap<String, String>,
    instance_properties: &HashMap<String, String>,
    protocol: &ProtocolHandler,
) -> v1beta1::ContainerAllocateResponse {
    let mut mounts: Vec<v1beta1::Mount> = Vec::new();

    // Set mounts according to protocol
    match protocol {
        ProtocolHandler::udev(_handler_config) => {
            trace!("get_volumes_and_mounts - setting volumes and mounts for udev protocol");
            mounts = instance_properties
                .iter()
                .map(|(_id, devpath)| v1beta1::Mount {
                    container_path: devpath.clone(),
                    host_path: devpath.clone(),
                    read_only: true,
                })
                .collect();
        }
        _ => trace!("get_volumes_and_mounts - no mounts or volumes required by this protocol"),
    }

    // Create response, setting environment variables to be an instance's properties (specified by protocol)
    v1beta1::ContainerAllocateResponse {
        annotations,
        mounts,
        envs: instance_properties.clone(),
        ..Default::default()
    }
}

/// Try to find Instance CRD for this instance or create one and add it to the global InstanceMap
/// If a Config does not exist for this instance, return error.
/// This is most likely caused by deletion of a Config right after adding it, in which case
/// `handle_config_delete` fails to delete this instance because kubelet has yet to call `list_and_watch`
async fn try_create_instance(
    dps: Arc<DevicePluginService>,
    kube_interface: Arc<impl KubeInterface>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
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
        .map(|x| (format!("{}-{}", dps.instance_name, x), "".to_string()))
        .collect();
    let instance = Instance {
        configuration_name: dps.config_name.clone(),
        shared: dps.shared,
        nodes: vec![dps.node_name.clone()],
        device_usage,
        metadata: dps.instance_properties.clone(),
        rbac: "rbac".to_string(),
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
                            &instance_object.metadata.name,
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
    dps.instance_map.lock().await.insert(
        dps.instance_name.clone(),
        InstanceInfo {
            list_and_watch_message_sender: dps.list_and_watch_message_sender.clone(),
            connectivity_status: ConnectivityStatus::Online,
        },
    );

    Ok(())
}

/// Returns list of "virtual" Devices and their health.
/// If the instance is offline, returns all unhealthy virtual Devices.
async fn build_list_and_watch_response(
    dps: Arc<DevicePluginService>,
    kube_interface: Arc<impl KubeInterface>,
) -> Result<Vec<v1beta1::Device>, Box<dyn std::error::Error + Send + Sync + 'static>> {
    info!(
        "build_list_and_watch_response -- for Instance {} entered",
        dps.instance_name
    );

    // If instance has been removed from map, send back all unhealthy device slots
    if !dps
        .instance_map
        .lock()
        .await
        .contains_key(&dps.instance_name)
    {
        trace!("build_list_and_watch_response - Instance {} removed from map ... returning unhealthy devices", dps.instance_name);
        return Ok(build_unhealthy_virtual_devices(
            dps.config.capacity,
            &dps.instance_name,
        ));
    }
    // If instance is offline, send back all unhealthy device slots
    if dps
        .instance_map
        .lock()
        .await
        .get(&dps.instance_name)
        .unwrap()
        .connectivity_status
        != ConnectivityStatus::Online
    {
        trace!("build_list_and_watch_response - device for Instance {} is offline ... returning unhealthy devices", dps.instance_name);
        return Ok(build_unhealthy_virtual_devices(
            dps.config.capacity,
            &dps.instance_name,
        ));
    }

    trace!(
        "build_list_and_watch_response -- device for Instance {} is online",
        dps.instance_name
    );

    match kube_interface
        .find_instance(&dps.instance_name, &dps.config_namespace)
        .await
    {
        Ok(kube_akri_instance) => Ok(build_virtual_devices(
            &kube_akri_instance.spec.device_usage,
            kube_akri_instance.spec.shared,
            &dps.node_name,
        )),
        Err(_) => {
            trace!("build_list_and_watch_response - could not find instance {} so returning unhealthy devices", dps.instance_name);
            Ok(build_unhealthy_virtual_devices(
                dps.config.capacity,
                &dps.instance_name,
            ))
        }
    }
}

/// This builds a list of unhealthy virtual Devices.
fn build_unhealthy_virtual_devices(capacity: i32, instance_name: &str) -> Vec<v1beta1::Device> {
    let mut devices: Vec<v1beta1::Device> = Vec::new();
    for x in 0..capacity {
        let device = v1beta1::Device {
            id: format!("{}-{}", instance_name, x),
            health: UNHEALTHY.to_string(),
        };
        trace!(
            "build_unhealthy_virtual_devices -- for Instance {} reporting unhealthy devices for device with name [{}] and health: [{}]",
            instance_name,
            device.id,
            device.health,
        );
        devices.push(device);
    }
    devices
}

/// This builds a list of virtual Devices, determining the health of each virtual Device as follows:
/// Healthy if it is available to be used by this node or Unhealthy if it is already taken by another node.
fn build_virtual_devices(
    device_usage: &HashMap<String, String>,
    shared: bool,
    node_name: &str,
) -> Vec<v1beta1::Device> {
    let mut devices: Vec<v1beta1::Device> = Vec::new();
    for (device_name, allocated_node) in device_usage {
        // Throw error if unshared resource is reserved by another node
        if !shared && allocated_node != "" && allocated_node != node_name {
            panic!("build_virtual_devices - unshared device reserved by a different node");
        }
        // Advertise the device as Unhealthy if it is
        // USED by !this_node && SHARED
        let unhealthy = shared && allocated_node != "" && allocated_node != node_name;
        let health = if unhealthy {
            UNHEALTHY.to_string()
        } else {
            HEALTHY.to_string()
        };
        trace!(
            "build_virtual_devices - [shared = {}] device with name [{}] and health: [{}]",
            shared,
            device_name,
            health
        );
        devices.push(v1beta1::Device {
            id: device_name.clone(),
            health,
        });
    }
    devices
}

/// This sends message to end `list_and_watch` and removes instance from InstanceMap.
/// Called when an instance has been offline for too long.
pub async fn terminate_device_plugin_service(
    instance_name: &str,
    instance_map: InstanceMap,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    let mut instance_map = instance_map.lock().await;
    trace!(
        "terminate_device_plugin_service -- forcing list_and_watch to end for Instance {}",
        instance_name
    );
    instance_map
        .get(instance_name)
        .unwrap()
        .list_and_watch_message_sender
        .send(ListAndWatchMessageKind::End)
        .unwrap();

    trace!(
        "terminate_device_plugin_service -- removing Instance {} from instance_map",
        instance_name
    );
    instance_map.remove(instance_name);
    Ok(())
}

/// This creates a new DevicePluginService for an instance and registers it with kubelet
pub async fn build_device_plugin(
    instance_name: String,
    config_name: String,
    config_uid: String,
    config_namespace: String,
    config: Configuration,
    shared: bool,
    instance_properties: HashMap<String, String>,
    instance_map: InstanceMap,
    device_plugin_path: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    info!("build_device_plugin - entered for device {}", instance_name);
    let capability_id: String = format!("{}/{}", AKRI_PREFIX, instance_name);
    let unique_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?;
    let device_endpoint: String = format!("{}-{}.sock", instance_name, unique_time.as_secs());
    let socket_path: String = Path::new(device_plugin_path)
        .join(device_endpoint.clone())
        .to_str()
        .unwrap()
        .to_string();
    // Channel capacity set to 6 because 3 possible senders (allocate, update_connectivity_status, and handle_config_delete)
    // and and receiver only periodically checks channel
    let (list_and_watch_message_sender, _) = broadcast::channel(6);
    // Channel capacity set to 2 because worst case both register and list_and_watch send messages at same time and receiver is always listening
    let (server_ender_sender, server_ender_receiver) = mpsc::channel(2);
    let device_plugin_service = DevicePluginService {
        instance_name: instance_name.clone(),
        endpoint: device_endpoint.clone(),
        config,
        config_name: config_name.clone(),
        config_uid: config_uid.clone(),
        config_namespace: config_namespace.clone(),
        shared,
        node_name: env::var("AGENT_NODE_NAME")?,
        instance_properties,
        instance_map: instance_map.clone(),
        list_and_watch_message_sender: list_and_watch_message_sender.clone(),
        server_ender_sender: server_ender_sender.clone(),
    };

    serve(
        device_plugin_service,
        socket_path.clone(),
        server_ender_receiver,
    )
    .await?;

    register(
        capability_id,
        device_endpoint,
        &instance_name,
        server_ender_sender,
    )
    .await?;

    Ok(())
}

/// This acts as a signal future to gracefully shutdown DevicePluginServer upon its completion.
/// Ends when it receives message from `list_and_watch`.
async fn shutdown_signal(mut server_ender_receiver: mpsc::Receiver<()>) {
    match server_ender_receiver.recv().await {
        Some(_) => trace!(
            "shutdown_signal - received signal ... device plugin service gracefully shutting down"
        ),
        None => trace!("shutdown_signal - connection to server_ender_sender closed ... error"),
    }
}

// This serves DevicePluginServer
async fn serve(
    device_plugin_service: DevicePluginService,
    socket_path: String,
    server_ender_receiver: mpsc::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    info!(
        "serve - creating a device plugin server that will listen at: {}",
        socket_path
    );
    tokio::fs::create_dir_all(Path::new(&socket_path[..]).parent().unwrap())
        .await
        .expect("Failed to create dir at socket path");
    let mut uds = UnixListener::bind(socket_path.clone()).expect("Failed to bind to socket path");
    let service = DevicePluginServer::new(device_plugin_service);
    let socket_path_to_delete = socket_path.clone();
    task::spawn(async move {
        Server::builder()
            .add_service(service)
            .serve_with_incoming_shutdown(
                uds.incoming().map_ok(unix::UnixStream),
                shutdown_signal(server_ender_receiver),
            )
            .await
            .unwrap();
        trace!(
            "serve - gracefully shutdown ... deleting socket {}",
            socket_path_to_delete
        );
        // Socket may already be deleted in the case of kubelet restart
        std::fs::remove_file(socket_path_to_delete).unwrap_or(());
    });

    // Test that server is running, trying for at most 10 seconds
    // Similar to grpc.timeout, which is yet to be implemented for tonic
    // See issue: https://github.com/hyperium/tonic/issues/75
    let mut connected = false;
    let start = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs();
    let start_plus_10 = start + 10;

    while (SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs()
        < start_plus_10)
        && !connected
    {
        let path = socket_path.clone();
        if let Ok(_v) = Endpoint::try_from("lttp://[::]:50051")?
            .connect_with_connector(service_fn(move |_: Uri| UnixStream::connect(path.clone())))
            .await
        {
            connected = true
        } else {
            delay_for(Duration::from_secs(1)).await
        }
    }

    if !connected {
        error!(
            "serve - could not connect to Device Plugin server on socket {}",
            socket_path
        );
    }
    Ok(())
}

/// This registers DevicePlugin with kubelet.
/// During registration, the device plugin must send
/// (1) name of unix socket,
/// (2) Device-Plugin API it was built against (v1beta1),
/// (3) resource name akri.sh/device_id.
/// If registration request to kubelet fails, terminates DevicePluginService.
async fn register(
    capability_id: String,
    socket_name: String,
    instance_name: &str,
    mut server_ender_sender: mpsc::Sender<()>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    info!(
        "register - entered for Instance {} and socket_name: {}",
        capability_id, socket_name
    );
    let op = DevicePluginOptions {
        pre_start_required: false,
    };

    // lttp://... is a fake uri that is unused (in service_fn) but necessary for uds connection
    let channel = Endpoint::try_from("lttp://[::]:50051")?
        .connect_with_connector(service_fn(|_: Uri| UnixStream::connect(KUBELET_SOCKET)))
        .await?;
    let mut registration_client = registration_client::RegistrationClient::new(channel);

    let register_request = tonic::Request::new(v1beta1::RegisterRequest {
        version: K8S_DEVICE_PLUGIN_VERSION.into(),
        endpoint: socket_name,
        resource_name: capability_id,
        options: Some(op),
    });
    trace!(
        "register - before call to register with Kubelet at socket {}",
        KUBELET_SOCKET
    );

    // If fail to register with kubelet, terminate device plugin
    if registration_client
        .register(register_request)
        .await
        .is_err()
    {
        trace!(
            "register - failed to register Instance {} with kubelet ... terminating device plugin",
            instance_name
        );
        server_ender_sender.send(()).await?;
    }
    Ok(())
}

/// This creates an Instance's unique name
pub fn get_device_instance_name(id: &str, config_name: &str) -> String {
    format!("{}-{}", config_name, &id)
        .replace(".", "-")
        .replace("/", "-")
}

/// Module to enable UDS with tonic grpc.
/// This is unix only since the underlying UnixStream and UnixListener libraries are unix only.
#[cfg(unix)]
mod unix {
    use std::{
        pin::Pin,
        task::{Context, Poll},
    };

    use tokio::io::{AsyncRead, AsyncWrite};
    use tonic::transport::server::Connected;

    #[derive(Debug)]
    pub struct UnixStream(pub tokio::net::UnixStream);

    impl Connected for UnixStream {}

    impl AsyncRead for UnixStream {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<std::io::Result<usize>> {
            Pin::new(&mut self.0).poll_read(cx, buf)
        }
    }

    impl AsyncWrite for UnixStream {
        fn poll_write(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<std::io::Result<usize>> {
            Pin::new(&mut self.0).poll_write(cx, buf)
        }

        fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Pin::new(&mut self.0).poll_flush(cx)
        }

        fn poll_shutdown(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<std::io::Result<()>> {
            Pin::new(&mut self.0).poll_shutdown(cx)
        }
    }
}

#[cfg(test)]
mod device_plugin_service_tests {
    use super::super::v1beta1::device_plugin_client::DevicePluginClient;
    use super::*;
    use akri_shared::akri::configuration::KubeAkriConfig;
    use akri_shared::{
        akri::instance::{Instance, KubeAkriInstance},
        k8s::MockKubeInterface,
    };
    use mockall::predicate::*;
    use std::{
        fs,
        io::{Error, ErrorKind},
    };
    use tempfile::Builder;

    enum NodeName {
        ThisNode,
        OtherNode,
    }

    // Need to be kept alive during tests
    struct DevicePluginServiceReceivers {
        list_and_watch_message_receiver: broadcast::Receiver<ListAndWatchMessageKind>,
        server_ender_receiver: mpsc::Receiver<()>,
    }

    fn configure_find_instance(
        mock: &mut MockKubeInterface,
        result_file: &'static str,
        instance_name: String,
        instance_namespace: String,
        device_usage_node: &'static str,
        node_name: NodeName,
    ) {
        let instance_name_clone = instance_name.clone();
        mock.expect_find_instance()
            .times(1)
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
                instance_json = instance_json.replace("node-a", &host_name);
                instance_json = instance_json.replace("config-a-b494b6", &instance_name_clone);
                instance_json =
                    instance_json.replace("\":\"\"", &format!("\":\"{}\"", device_usage_node));
                let instance: KubeAkriInstance = serde_json::from_str(&instance_json).unwrap();
                Ok(instance)
            });
    }

    fn create_device_plugin_service(
        connectivity_status: ConnectivityStatus,
        add_to_instance_map: bool,
    ) -> (DevicePluginService, DevicePluginServiceReceivers) {
        let path_to_config = "../test/json/config-a.json";
        let kube_akri_config_json =
            fs::read_to_string(path_to_config).expect("Unable to read file");
        let kube_akri_config: KubeAkriConfig =
            serde_json::from_str(&kube_akri_config_json).unwrap();
        let device_instance_name =
            get_device_instance_name("b494b6", &kube_akri_config.metadata.name);
        let unique_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH);
        let device_endpoint: String = format!(
            "{}-{}.sock",
            device_instance_name,
            unique_time.unwrap_or_default().as_secs()
        );
        let (list_and_watch_message_sender, list_and_watch_message_receiver) =
            broadcast::channel(4);
        let (server_ender_sender, server_ender_receiver) = mpsc::channel(1);

        let mut map = HashMap::new();
        if add_to_instance_map {
            let instance_info: InstanceInfo = InstanceInfo {
                list_and_watch_message_sender: list_and_watch_message_sender.clone(),
                connectivity_status,
            };
            map.insert(device_instance_name.clone(), instance_info);
        }
        let instance_map: InstanceMap = Arc::new(Mutex::new(map));

        let dps = DevicePluginService {
            instance_name: device_instance_name,
            endpoint: device_endpoint,
            config: kube_akri_config.spec.clone(),
            config_name: kube_akri_config.metadata.name,
            config_uid: kube_akri_config.metadata.uid.unwrap(),
            config_namespace: kube_akri_config.metadata.namespace.unwrap(),
            shared: false,
            node_name: "node-a".to_string(),
            instance_properties: HashMap::new(),
            instance_map,
            list_and_watch_message_sender,
            server_ender_sender,
        };
        (
            dps,
            DevicePluginServiceReceivers {
                list_and_watch_message_receiver,
                server_ender_receiver,
            },
        )
    }

    fn check_devices(instance_name: String, devices: Vec<v1beta1::Device>) {
        let capacity: usize = 5;
        // update_virtual_devices_health returns devices in jumbled order (ie 2, 4, 1, 5, 3)
        let expected_device_ids: Vec<String> = (0..capacity)
            .map(|x| format!("{}-{}", instance_name, x))
            .collect();
        assert_eq!(devices.len(), capacity);
        // Can't use map on Device type
        let device_ids: Vec<String> = devices.into_iter().map(|device| device.id).collect();
        for device in expected_device_ids {
            assert!(device_ids.contains(&device));
        }
    }

    // Tests that instance names are formatted correctly
    #[test]
    fn test_get_device_instance_name() {
        let instance_name1: String = "/dev/video0".to_string();
        let instance_name2: String = "10.1.2.3".to_string();
        assert_eq!(
            "usb-camera--dev-video0",
            get_device_instance_name(&instance_name1, &"usb-camera".to_string())
        );
        assert_eq!(
            "ip-camera-10-1-2-3".to_string(),
            get_device_instance_name(&instance_name2, &"ip-camera".to_string())
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
                let path_to_config = "../test/json/config-a.json";
                let kube_akri_config_json =
                    fs::read_to_string(path_to_config).expect("Unable to read file");
                let kube_akri_config: KubeAkriConfig =
                    serde_json::from_str(&kube_akri_config_json).unwrap();
                Ok(kube_akri_config)
            });
    }

    // Tests that try_create_instance creates an instance
    #[tokio::test]
    async fn test_try_create_instance() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (device_plugin_service, _device_plugin_service_receivers) =
            create_device_plugin_service(ConnectivityStatus::Online, false);
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
            .returning(move |_, _| {
                let error = Error::new(ErrorKind::InvalidInput, "Configuration doesn't exist");
                Err(Box::new(error))
            });
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
        assert!(try_create_instance(dps.clone(), Arc::new(mock))
            .await
            .is_ok());
        assert!(dps
            .instance_map
            .lock()
            .await
            .contains_key(&dps.instance_name));
    }

    // Tests that try_create_instance updates already existing instance with this node
    #[tokio::test]
    async fn test_try_create_instance_already_created() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (device_plugin_service, _device_plugin_service_receivers) =
            create_device_plugin_service(ConnectivityStatus::Online, false);
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
            "",
            NodeName::OtherNode,
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
        assert!(try_create_instance(dps.clone(), Arc::new(mock))
            .await
            .is_ok());
        assert!(dps
            .instance_map
            .lock()
            .await
            .contains_key(&dps.instance_name));
    }

    // Test when instance already created and already contains this node.
    // Should find the instance but not update it.
    #[tokio::test]
    async fn test_try_create_instance_already_created_no_update() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (device_plugin_service, _device_plugin_service_receivers) =
            create_device_plugin_service(ConnectivityStatus::Online, false);
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
            "",
            NodeName::ThisNode,
        );
        let dps = Arc::new(device_plugin_service);
        assert!(try_create_instance(dps.clone(), Arc::new(mock))
            .await
            .is_ok());
        assert!(dps
            .instance_map
            .lock()
            .await
            .contains_key(&dps.instance_name));
    }

    // Tests that try_create_instance returns error when trying to create an Instance for a Config that DNE
    #[tokio::test]
    async fn test_try_create_instance_no_config() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (device_plugin_service, _device_plugin_service_receivers) =
            create_device_plugin_service(ConnectivityStatus::Online, false);
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
                Err(Box::new(error))
            });
        assert!(
            try_create_instance(Arc::new(device_plugin_service), Arc::new(mock))
                .await
                .is_err()
        );
    }

    // Tests that try_create_instance error
    #[tokio::test]
    async fn test_try_create_instance_error() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (device_plugin_service, _device_plugin_service_receivers) =
            create_device_plugin_service(ConnectivityStatus::Online, false);
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
            .returning(move |_, _| Err(None.ok_or("failure")?));
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
            .returning(move |_, _, _, _, _| Err(None.ok_or("failure")?));

        let dps = Arc::new(device_plugin_service);
        assert!(try_create_instance(dps.clone(), Arc::new(mock))
            .await
            .is_err());
        assert!(!dps
            .instance_map
            .lock()
            .await
            .contains_key(&dps.instance_name));
    }

    // Tests list_and_watch by creating DevicePluginService and DevicePlugin client (emulating kubelet)
    #[tokio::test]
    async fn test_list_and_watch() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (device_plugin_service, device_plugin_service_receivers) =
            create_device_plugin_service(ConnectivityStatus::Online, false);
        let device_plugin_temp_dir = Builder::new().prefix("device-plugins-").tempdir().unwrap();
        let socket_path: String = device_plugin_temp_dir
            .path()
            .join(device_plugin_service.endpoint.clone())
            .to_str()
            .unwrap()
            .to_string();
        let list_and_watch_message_sender =
            device_plugin_service.list_and_watch_message_sender.clone();
        let instance_name = device_plugin_service.instance_name.clone();
        serve(
            device_plugin_service,
            socket_path.clone(),
            device_plugin_service_receivers.server_ender_receiver,
        )
        .await
        .unwrap();
        let channel = Endpoint::try_from("lttp://[::]:50051")
            .unwrap()
            .connect_with_connector(service_fn(move |_: Uri| {
                UnixStream::connect(socket_path.clone())
            }))
            .await
            .unwrap();
        let mut client = DevicePluginClient::new(channel);
        let mut stream = client
            .list_and_watch(Request::new(Empty {}))
            .await
            .unwrap()
            .into_inner();
        list_and_watch_message_sender
            .send(ListAndWatchMessageKind::End)
            .unwrap();
        if let Some(list_and_watch_response) = stream.message().await.unwrap() {
            assert_eq!(
                list_and_watch_response.devices[0].id,
                format!("{}-0", instance_name)
            );
        };
    }

    #[tokio::test]
    async fn test_build_virtual_devices() {
        let mut device_usage: HashMap<String, String> = HashMap::new();
        let mut expected_devices_nodea: HashMap<String, String> = HashMap::new();
        let mut expected_devices_nodeb: HashMap<String, String> = HashMap::new();
        let instance_name = "s0meH@sH";
        for x in 0..5 {
            if x % 2 == 0 {
                device_usage.insert(format!("{}-{}", instance_name, x), "nodeA".to_string());
                expected_devices_nodea
                    .insert(format!("{}-{}", instance_name, x), HEALTHY.to_string());
                expected_devices_nodeb
                    .insert(format!("{}-{}", instance_name, x), UNHEALTHY.to_string());
            } else {
                device_usage.insert(format!("{}-{}", instance_name, x), "".to_string());
                expected_devices_nodea
                    .insert(format!("{}-{}", instance_name, x), HEALTHY.to_string());
                expected_devices_nodeb
                    .insert(format!("{}-{}", instance_name, x), HEALTHY.to_string());
            }
        }

        // Test shared all healthy
        let mut devices: Vec<v1beta1::Device> =
            build_virtual_devices(&device_usage, true, &"nodeA".to_string());
        for device in devices {
            assert_eq!(
                expected_devices_nodea.get(&device.id).unwrap(),
                &device.health
            );
        }

        // Test unshared all healthy
        devices = build_virtual_devices(&device_usage, false, &"nodeA".to_string());
        for device in devices {
            assert_eq!(
                expected_devices_nodea.get(&device.id).unwrap(),
                &device.health
            );
        }

        // Test shared some unhealthy (taken by another node)
        devices = build_virtual_devices(&device_usage, true, &"nodeB".to_string());
        for device in devices {
            assert_eq!(
                expected_devices_nodeb.get(&device.id).unwrap(),
                &device.health
            );
        }

        // Test unshared panic. A different node should never be listed under any device usage slots
        let result = std::panic::catch_unwind(|| {
            build_virtual_devices(&device_usage, false, &"nodeB".to_string())
        });
        assert!(result.is_err());
    }

    // Tests when ConnectivityStatus is offline and unhealthy devices are returned
    #[tokio::test]
    async fn test_build_list_and_watch_response_offline() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (device_plugin_service, _device_plugin_service_receivers) =
            create_device_plugin_service(ConnectivityStatus::Offline(Instant::now()), true);
        let mock = MockKubeInterface::new();
        let devices =
            build_list_and_watch_response(Arc::new(device_plugin_service), Arc::new(mock))
                .await
                .unwrap();
        devices
            .into_iter()
            .for_each(|device| assert!(device.health == UNHEALTHY));
    }

    // Tests when instance has not yet been created for this device, all devices are returned as UNHEALTHY
    #[tokio::test]
    async fn test_build_list_and_watch_response_no_instance() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (device_plugin_service, _device_plugin_service_receivers) =
            create_device_plugin_service(ConnectivityStatus::Online, true);
        let instance_name = device_plugin_service.instance_name.clone();
        let instance_namespace = device_plugin_service.config_namespace.clone();
        let mut mock = MockKubeInterface::new();
        mock.expect_find_instance()
            .times(1)
            .withf(move |name: &str, namespace: &str| {
                namespace == instance_namespace && name == instance_name
            })
            .returning(move |_, _| {
                let error = Error::new(ErrorKind::InvalidInput, "Instance doesn't exist");
                Err(Box::new(error))
            });
        let devices =
            build_list_and_watch_response(Arc::new(device_plugin_service), Arc::new(mock))
                .await
                .unwrap();
        devices
            .into_iter()
            .for_each(|device| assert!(device.health == UNHEALTHY));
    }

    // Test when instance has already been created and includes this node
    #[tokio::test]
    async fn test_build_list_and_watch_response_no_instance_update() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (device_plugin_service, _device_plugin_service_receivers) =
            create_device_plugin_service(ConnectivityStatus::Online, true);
        let instance_name = device_plugin_service.instance_name.clone();
        let instance_namespace = device_plugin_service.config_namespace.clone();
        let mut mock = MockKubeInterface::new();
        configure_find_instance(
            &mut mock,
            "../test/json/local-instance.json",
            instance_name.clone(),
            instance_namespace.clone(),
            "",
            NodeName::ThisNode,
        );
        let devices =
            build_list_and_watch_response(Arc::new(device_plugin_service), Arc::new(mock))
                .await
                .unwrap();
        check_devices(instance_name, devices);
    }

    // Test when device_usage[id] == ""
    // internal_allocate should set device_usage[id] = m.nodeName, return
    #[tokio::test]
    async fn test_internal_allocate_success() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (device_plugin_service, mut device_plugin_service_receivers) =
            create_device_plugin_service(ConnectivityStatus::Online, true);
        let device_usage_id_slot = format!("{}-0", device_plugin_service.instance_name);
        let device_usage_id_slot_2 = device_usage_id_slot.clone();
        let node_name = device_plugin_service.node_name.clone();
        let mut mock = MockKubeInterface::new();
        configure_find_instance(
            &mut mock,
            "../test/json/local-instance.json",
            device_plugin_service.instance_name.clone(),
            device_plugin_service.config_namespace.clone(),
            "",
            NodeName::ThisNode,
        );
        mock.expect_update_instance()
            .times(1)
            .withf(move |instance_to_update: &Instance, _, _| {
                instance_to_update
                    .device_usage
                    .get(&device_usage_id_slot)
                    .unwrap()
                    == &node_name
            })
            .returning(move |_, _, _| Ok(()));
        let devices_i_ds = vec![device_usage_id_slot_2];
        let container_requests = vec![v1beta1::ContainerAllocateRequest { devices_i_ds }];
        let requests = Request::new(AllocateRequest { container_requests });
        assert!(device_plugin_service
            .internal_allocate(requests, Arc::new(mock),)
            .await
            .is_ok());
        assert!(device_plugin_service_receivers
            .list_and_watch_message_receiver
            .try_recv()
            .is_err());
    }

    // Test when device_usage[id] == self.nodeName
    // Expected behavior: internal_allocate should set device_usage[id] == "", invoke list_and_watch, and return error
    #[tokio::test]
    async fn test_internal_allocate_deallocate() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (device_plugin_service, mut device_plugin_service_receivers) =
            create_device_plugin_service(ConnectivityStatus::Online, true);
        let device_usage_id_slot = format!("{}-0", device_plugin_service.instance_name);
        let device_usage_id_slot_2 = device_usage_id_slot.clone();
        let mut mock = MockKubeInterface::new();
        configure_find_instance(
            &mut mock,
            "../test/json/local-instance.json",
            device_plugin_service.instance_name.clone(),
            device_plugin_service.config_namespace.clone(),
            "node-a",
            NodeName::ThisNode,
        );
        mock.expect_update_instance()
            .times(1)
            .withf(move |instance_to_update: &Instance, _, _| {
                instance_to_update
                    .device_usage
                    .get(&device_usage_id_slot)
                    .unwrap()
                    == ""
            })
            .returning(move |_, _, _| Ok(()));
        let devices_i_ds = vec![device_usage_id_slot_2];
        let container_requests = vec![v1beta1::ContainerAllocateRequest { devices_i_ds }];
        let requests = Request::new(AllocateRequest { container_requests });
        match device_plugin_service
            .internal_allocate(requests, Arc::new(mock))
            .await
        {
            Ok(_) => {
                panic!("internal allocate is expected to fail due to devices being in bad state")
            }
            Err(e) => assert_eq!(
                e.message(),
                "Devices are in inconsistent state, updated device usage, please retry scheduling"
            ),
        }
        assert_eq!(
            device_plugin_service_receivers
                .list_and_watch_message_receiver
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
            create_device_plugin_service(ConnectivityStatus::Online, true);
        let device_usage_id_slot = format!("{}-0", device_plugin_service.instance_name);
        let mut mock = MockKubeInterface::new();
        configure_find_instance(
            &mut mock,
            "../test/json/local-instance.json",
            device_plugin_service.instance_name.clone(),
            device_plugin_service.config_namespace.clone(),
            "other",
            NodeName::ThisNode,
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
            Err(e) => assert_eq!(e.message(), "Requested device already in use"),
        }
        assert_eq!(
            device_plugin_service_receivers
                .list_and_watch_message_receiver
                .recv()
                .await
                .unwrap(),
            ListAndWatchMessageKind::Continue
        );
    }

    // Tests when instance does not have the requested device usage id
    // Expected behavior: should invoke list_and_watch, and return error
    #[tokio::test]
    async fn test_internal_allocate_no_id() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (device_plugin_service, mut device_plugin_service_receivers) =
            create_device_plugin_service(ConnectivityStatus::Online, true);
        let device_usage_id_slot = format!("{}-100", device_plugin_service.instance_name);
        let mut mock = MockKubeInterface::new();
        configure_find_instance(
            &mut mock,
            "../test/json/local-instance.json",
            device_plugin_service.instance_name.clone(),
            device_plugin_service.config_namespace.clone(),
            "other",
            NodeName::ThisNode,
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
                .list_and_watch_message_receiver
                .recv()
                .await
                .unwrap(),
            ListAndWatchMessageKind::Continue
        );
    }
}
