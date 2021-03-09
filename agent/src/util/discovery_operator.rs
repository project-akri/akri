use super::super::INSTANCE_COUNT_METRIC;
use super::{
    constants::{
        DISCOVERY_HANDLER_OFFLINE_GRACE_PERIOD_SECS, SHARED_INSTANCE_OFFLINE_GRACE_PERIOD_SECS,
    },
    device_plugin_builder::{DevicePluginBuilder, DevicePluginBuilderInterface},
    device_plugin_service,
    device_plugin_service::{
        get_device_instance_name, InstanceConnectivityStatus, InstanceInfo, InstanceMap,
    },
    embedded_discovery_handlers::get_discovery_handler,
    registration::{
        DiscoveryDetails, DiscoveryHandlerEndpoint, DiscoveryHandlerStatus,
        RegisteredDiscoveryHandlerMap,
    },
    streaming_extension::StreamingExt,
};
use akri_discovery_utils::discovery::v0::{
    discovery_handler_client::DiscoveryHandlerClient, Device, DiscoverRequest, DiscoverResponse,
};
use akri_shared::{
    akri::configuration::KubeAkriConfig,
    k8s,
    os::env_var::{ActualEnvVarQuery, EnvVarQuery},
};
use blake2::{
    digest::{Update, VariableOutput},
    VarBlake2b,
};
use log::{error, trace};
#[cfg(test)]
use mock_instant::Instant;
#[cfg(test)]
use mockall::{automock, predicate::*};
#[cfg(not(test))]
use std::time::Instant;
use std::{collections::HashMap, convert::TryFrom, sync::Arc};
use tokio::sync::mpsc;
use tonic::{
    transport::{Endpoint, Uri},
    Status,
};

/// StreamType provides a wrapper around the two different types of streams returned from embedded
/// or embedded discovery handlers and ones running externally.
pub enum StreamType {
    Embedded(mpsc::Receiver<std::result::Result<DiscoverResponse, Status>>),
    External(tonic::Streaming<DiscoverResponse>),
}

/// A DiscoveryOperator is created for each Configuration that is applied to the cluster.
/// It handles discovery of the devices specified in a Configuration by calling `Discover` on
/// all `DiscoveryHandlers` registered with name `Configuration.discovery_handler.name.`
/// For each device discovered by the discovery handlers, it creates a device plugin.
/// If a device disappears, it deletes the associated instance after a grace period (for non-local devices).
/// Note: Since this structure is automocked, the compiler does not seem to be able to confirm that all the
/// methods are being used. Therefore, #[allow(dead_code)] has been added to all methods that are not invoked or
/// tested on a DiscoveryOperator.
#[derive(Clone)]
pub struct DiscoveryOperator {
    /// Map of registered discovery handlers
    discovery_handler_map: RegisteredDiscoveryHandlerMap,
    /// The Akri Configuration associated with this `DiscoveryOperator`.
    /// The Configuration tells the `DiscoveryOperator` what to look for.
    config: KubeAkriConfig,
    /// Map of Akri Instances discovered by this `DiscoveryOperator`
    instance_map: InstanceMap,
}

#[cfg_attr(test, automock)]
impl DiscoveryOperator {
    pub fn new(
        discovery_handler_map: RegisteredDiscoveryHandlerMap,
        config: KubeAkriConfig,
        instance_map: InstanceMap,
    ) -> Self {
        DiscoveryOperator {
            discovery_handler_map,
            config,
            instance_map,
        }
    }
    /// Returns discovery_handler_map field. Allows the struct to be mocked.
    #[allow(dead_code)]
    pub fn get_discovery_handler_map(&self) -> RegisteredDiscoveryHandlerMap {
        self.discovery_handler_map.clone()
    }
    /// Returns config field. Allows the struct to be mocked.
    #[allow(dead_code)]
    pub fn get_config(&self) -> KubeAkriConfig {
        self.config.clone()
    }
    /// Returns instance_map field. Allows the struct to be mocked.
    #[allow(dead_code)]
    pub fn get_instance_map(&self) -> InstanceMap {
        self.instance_map.clone()
    }
    #[allow(dead_code)]
    pub async fn stop_all_discovery(&self) {
        let mut discovery_handler_map = self.discovery_handler_map.lock().unwrap().clone();
        if let Some(discovery_handler_details_map) =
            discovery_handler_map.get_mut(&self.config.spec.discovery_handler.name)
        {
            for (endpoint, dh_details) in discovery_handler_details_map.clone() {
                match dh_details.close_discovery_handler_connection.send(()) {
                    Ok(_) => trace!("stop_all_discovery - discovery client for {} discovery handler at endpoint {:?} told to stop", self.config.spec.discovery_handler.name, endpoint),
                    Err(e) => error!("stop_all_discovery - discovery client {} discovery handler at endpoint {:?} could not receive stop message with error {:?}", self.config.spec.discovery_handler.name, endpoint, e)
                }
            }
        }
    }

    /// Calls discover on the Discovery Handler at the given endpoint and returns the connection stream.
    pub async fn get_stream(&self, endpoint: &DiscoveryHandlerEndpoint) -> Option<StreamType> {
        let discover_request = tonic::Request::new(DiscoverRequest {
            discovery_details: self.config.spec.discovery_handler.discovery_details.clone(),
        });
        trace!("get_stream - endpoint is {:?}", endpoint);
        match endpoint {
            DiscoveryHandlerEndpoint::Embedded => {
                match get_discovery_handler(&self.config.spec.discovery_handler) {
                    Ok(discovery_handler) => {
                        trace!(
                            "get_stream - using embedded {} discovery handler",
                            self.config.spec.discovery_handler.name
                        );
                        match discovery_handler.discover(discover_request).await {
                            Ok(device_update_receiver) => {
                                Some(StreamType::Embedded(device_update_receiver.into_inner()))
                            }
                            Err(e) => {
                                error!("get_stream - could not connect to DiscoveryHandler at endpoint {:?} with error {}", endpoint, e);
                                None
                            }
                        }
                    }
                    Err(e) => {
                        error!("get_stream - no embedded discovery handler found with name {} with error {:?}", self.config.spec.discovery_handler.name, e);
                        None
                    }
                }
            }
            DiscoveryHandlerEndpoint::Uds(socket) => {
                // Clone socket for closure which has static lifetime
                let socket = socket.clone();
                // We will ignore this dummy uri because UDS does not use it.
                match Endpoint::try_from("dummy://[::]:50051")
                    .unwrap()
                    .connect_with_connector(tower::service_fn(move |_: Uri| {
                        let endpoint = socket.clone();
                        tokio::net::UnixStream::connect(endpoint)
                    }))
                    .await
                {
                    Ok(channel) => {
                        trace!(
                            "get_stream - connecting to external {} discovery handler over UDS",
                            self.config.spec.discovery_handler.name
                        );
                        let mut discovery_handler_client = DiscoveryHandlerClient::new(channel);
                        match discovery_handler_client.discover(discover_request).await {
                            Ok(device_update_receiver) => {
                                Some(StreamType::External(device_update_receiver.into_inner()))
                            }
                            Err(e) => {
                                error!("get_stream - could not connect to DiscoveryHandler at endpoint {:?} with error {}", endpoint, e);
                                None
                            }
                        }
                    }
                    Err(e) => {
                        error!("get_stream - failed to connect to {} discovery handler over UDS with error {}", self.config.spec.discovery_handler.name, e);
                        None
                    }
                }
            }
            DiscoveryHandlerEndpoint::Network(addr) => {
                match DiscoveryHandlerClient::connect(addr.clone()).await {
                    Ok(mut discovery_handler_client) => {
                        trace!(
                            "get_stream - connecting to external {} discovery handler over network",
                            self.config.spec.discovery_handler.name
                        );
                        match discovery_handler_client.discover(discover_request).await {
                            Ok(device_update_receiver) => {
                                Some(StreamType::External(device_update_receiver.into_inner()))
                            }
                            Err(e) => {
                                error!("get_stream - could not connect to DiscoveryHandler at endpoint {:?} with error {}", endpoint, e);
                                None
                            }
                        }
                    }
                    Err(e) => {
                        error!("get_stream - failed to connect to {} discovery handler over network with error {}", self.config.spec.discovery_handler.name, e);
                        None
                    }
                }
            }
        }
    }
    /// Listens for new discovery responses and calls a function to handle the new discovery results.
    /// Runs until the future is canceled by the calling function upon notification to stop discovery.
    #[allow(dead_code)]
    pub async fn internal_do_discover<'a>(
        &'a self,
        kube_interface: Arc<Box<dyn k8s::KubeInterface>>,
        dh_details: &'a DiscoveryDetails,
        stream: &'a mut dyn StreamingExt,
    ) -> Result<(), Status> {
        loop {
            // Wait for either new discovery results or a message to stop discovery
            let result = stream.get_message().await;
            let message = result?;
            if let Some(response) = message {
                trace!(
                    "internal_do_discover - got discovery results {:?}",
                    response.devices
                );
                self.handle_discovery_results(
                    kube_interface.clone(),
                    response.devices,
                    dh_details.shared,
                    Box::new(DevicePluginBuilder {}),
                )
                .await
                .unwrap();
            } else {
                error!("internal_do_discover - received result of type None. Should not happen.");
                break;
            }
        }

        Ok(())
    }

    /// Sets the connectivity status of a discovery handler. If a discovery handler goes offline, mark_offline_or_deregister_discovery_handler should be used.
    pub fn set_discovery_handler_connectivity_status(
        &self,
        endpoint: &DiscoveryHandlerEndpoint,
        connectivity_status: DiscoveryHandlerStatus,
    ) {
        trace!("set_discovery_handler_connectivity_status - set status of {:?} for {} discovery handler at endpoint {:?}", connectivity_status, self.config.spec.discovery_handler.name, endpoint);
        let mut registered_dh_map = self.discovery_handler_map.lock().unwrap();
        let discovery_handler_details_map = registered_dh_map
            .get_mut(&self.config.spec.discovery_handler.name)
            .unwrap();
        let dh_details = discovery_handler_details_map.get_mut(endpoint).unwrap();
        dh_details.connectivity_status = connectivity_status;
    }

    /// This is called when no connection can be made with a discovery handler at its endpoint.
    /// It takes action based on a Discovery Handler's (DH's) current `DiscoveryHandlerStatus`.
    /// If `DiscoveryHandlerStatus::Waiting`, connectivity status changed to Offline.
    /// If `DiscoveryHandlerStatus::Offline`, DH is removed from the `RegisteredDiscoveryHandlersMap`
    /// if it have been offline for longer than the grace period.
    /// If `DiscoveryHandlerStatus::Active`, this should not happen, Error is returned.
    pub async fn mark_offline_or_deregister_discovery_handler(
        &self,
        endpoint: &DiscoveryHandlerEndpoint,
    ) -> Result<bool, anyhow::Error> {
        trace!("mark_offline_or_deregister_discovery_handler - {} discovery handler at endpoint {:?} is offline", self.config.spec.discovery_handler.name, endpoint);
        let mut deregistered = false;
        let mut registered_dh_map = self.discovery_handler_map.lock().unwrap();
        let discovery_handler_details_map = registered_dh_map
            .get_mut(&self.config.spec.discovery_handler.name)
            .unwrap();
        let dh_details = discovery_handler_details_map.get_mut(endpoint).unwrap();
        match dh_details.connectivity_status {
            DiscoveryHandlerStatus::Offline(instant) => {
                if instant.elapsed().as_secs() > DISCOVERY_HANDLER_OFFLINE_GRACE_PERIOD_SECS {
                    trace!("mark_offline_or_deregister_discovery_handler - de-registering {} discovery handler at endpoint {:?} since been offline for longer than 5 minutes", self.config.spec.discovery_handler.name, endpoint);
                    // Remove discovery handler from map if timed out
                    discovery_handler_details_map.remove(endpoint).unwrap();
                    deregistered = true;
                }
            }
            DiscoveryHandlerStatus::Waiting | DiscoveryHandlerStatus::Active => {
                dh_details.connectivity_status = DiscoveryHandlerStatus::Offline(Instant::now());
            }
        }
        Ok(deregistered)
    }

    /// Checks if any of this DiscoveryOperator's Configuration's Instances have been offline for too long.
    /// If a non-local device has not come back online before `SHARED_INSTANCE_OFFLINE_GRACE_PERIOD_SECS`,
    /// the associated Device Plugin and Instance are terminated and deleted, respectively.
    pub async fn delete_offline_instances(
        &self,
        kube_interface: Arc<Box<dyn k8s::KubeInterface>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        trace!(
            "delete_offline_instances - entered for configuration {}",
            self.config.metadata.name
        );
        let kube_interface_clone = kube_interface.clone();
        let instance_map = self.instance_map.lock().await.clone();
        for (instance, instance_info) in instance_map.clone() {
            if let InstanceConnectivityStatus::Offline(instant) = instance_info.connectivity_status
            {
                let time_offline = instant.elapsed().as_secs();
                // If instance has been offline for longer than the grace period or it is unshared, terminate the associated device plugin
                // TODO: make grace period configurable
                if time_offline >= SHARED_INSTANCE_OFFLINE_GRACE_PERIOD_SECS {
                    trace!("delete_offline_instances - instance {} has been offline too long ... terminating device plugin", instance);
                    device_plugin_service::terminate_device_plugin_service(
                        &instance,
                        self.instance_map.clone(),
                    )
                    .await
                    .unwrap();
                    k8s::try_delete_instance(
                        (*kube_interface_clone).as_ref(),
                        &instance,
                        self.config.metadata.namespace.as_ref().unwrap(),
                    )
                    .await?;
                }
            }
        }
        Ok(())
    }

    /// Takes in a list of discovered devices and determines if there are any new devices or no longer visible devices.
    /// For each new device, it creates a DevicePluginService.
    /// For each previously visible device that was no longer discovered, it calls a function that updates the InstanceConnectivityStatus
    /// of the instance or deletes it if it is a local device.
    pub async fn handle_discovery_results(
        &self,
        kube_interface: Arc<Box<dyn k8s::KubeInterface>>,
        discovery_results: Vec<Device>,
        shared: bool,
        device_plugin_builder: Box<dyn DevicePluginBuilderInterface>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        trace!(
            "handle_discovery_results - for config {} with discovery results {:?}",
            self.config.metadata.name,
            discovery_results
        );
        let currently_visible_instances: HashMap<String, Device> = discovery_results
            .iter()
            .map(|discovery_result| {
                let id = generate_instance_digest(&discovery_result.id, shared);
                let instance_name = get_device_instance_name(&id, &self.config.metadata.name);
                (instance_name, discovery_result.clone())
            })
            .collect();
        INSTANCE_COUNT_METRIC
            .with_label_values(&[&self.config.metadata.name, &shared.to_string()])
            .set(currently_visible_instances.len() as i64);
        // Update the connectivity status of instances and return list of visible instances that don't have Instance CRs
        let instance_map = self.instance_map.lock().await.clone();
        // Find all visible instances that do not have Instance CRDs yet
        let new_discovery_results: Vec<Device> = currently_visible_instances
            .iter()
            .filter(|(name, _)| !instance_map.contains_key(*name))
            .map(|(_, p)| p.clone())
            .collect();
        self.update_instance_connectivity_status(
            kube_interface,
            currently_visible_instances,
            shared,
        )
        .await?;

        // If there are newly visible instances associated with a Config, make a device plugin and Instance CR for them
        if !new_discovery_results.is_empty() {
            for discovery_result in new_discovery_results {
                let id = generate_instance_digest(&discovery_result.id, shared);
                let instance_name = get_device_instance_name(&id, &self.config.metadata.name);
                trace!(
                    "handle_discovery_results - new instance {} came online",
                    instance_name
                );
                let instance_map = self.instance_map.clone();
                if let Err(e) = device_plugin_builder
                    .build_device_plugin(
                        instance_name,
                        &self.config,
                        shared,
                        instance_map,
                        discovery_result.clone(),
                    )
                    .await
                {
                    error!("handle_discovery_results - error {} building device plugin ... trying again on next iteration", e);
                }
            }
        }
        Ok(())
    }

    /// Takes in a list of currently visible instances and either updates an Instance's InstanceConnectivityStatus or deletes an Instance.
    /// If a non-local/network based device is not longer visible it's InstanceConnectivityStatus is changed to Offline(time now).
    /// The associated DevicePluginService checks its InstanceConnectivityStatus before sending a response back to kubelet
    /// and will send all unhealthy devices if its status is Offline, preventing kubelet from allocating any more pods to it.
    /// An Instance CRD is deleted and it's DevicePluginService shutdown if its:
    /// (A) non-local Instance is still not visible after 5 minutes or (B) local instance is still not visible.
    pub async fn update_instance_connectivity_status(
        &self,
        kube_interface: Arc<Box<dyn k8s::KubeInterface>>,
        currently_visible_instances: HashMap<String, Device>,
        shared: bool,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        let instance_map = self.instance_map.lock().await.clone();
        for (instance, instance_info) in instance_map {
            trace!(
                "update_instance_connectivity_status - checking connectivity status of instance {}",
                instance
            );
            if currently_visible_instances.contains_key(&instance) {
                let connectivity_status = instance_info.connectivity_status;
                // If instance is visible, make sure connectivity status is (updated to be) Online
                if let InstanceConnectivityStatus::Offline(_instant) = connectivity_status {
                    trace!(
                        "update_instance_connectivity_status - instance {} that was temporarily offline is back online",
                        instance
                    );
                    let list_and_watch_message_sender = instance_info.list_and_watch_message_sender;
                    let updated_instance_info = InstanceInfo {
                        connectivity_status: InstanceConnectivityStatus::Online,
                        list_and_watch_message_sender: list_and_watch_message_sender.clone(),
                    };
                    self.instance_map
                        .lock()
                        .await
                        .insert(instance.clone(), updated_instance_info);
                    // Signal list_and_watch to update kubelet that the devices are healthy.
                    list_and_watch_message_sender
                        .send(device_plugin_service::ListAndWatchMessageKind::Continue)
                        .unwrap();
                } else {
                    trace!(
                        "update_instance_connectivity_status - instance {} still online",
                        instance
                    );
                }
            } else {
                // If the instance is not visible:
                // // If the instance is local, remove it
                // // If the instance is not local
                // // // If it has not already been labeled offline, label it
                // // // If the instance has already been labeled offline
                // // // remove instance from map if grace period has elapsed without the instance coming back online
                let mut remove_instance = false;
                match instance_info.connectivity_status {
                    InstanceConnectivityStatus::Online => {
                        if !shared {
                            remove_instance = true;
                        } else {
                            let sender = instance_info.list_and_watch_message_sender.clone();
                            let updated_instance_info = InstanceInfo {
                                connectivity_status: InstanceConnectivityStatus::Offline(
                                    Instant::now(),
                                ),
                                list_and_watch_message_sender: instance_info
                                    .list_and_watch_message_sender
                                    .clone(),
                            };
                            self.instance_map
                                .lock()
                                .await
                                .insert(instance.clone(), updated_instance_info);
                            trace!(
                                "update_instance_connectivity_status - instance {} went offline ... starting timer and forcing list_and_watch to continue",
                                instance
                            );
                            sender
                                .send(device_plugin_service::ListAndWatchMessageKind::Continue)
                                .unwrap();
                        }
                    }
                    InstanceConnectivityStatus::Offline(instant) => {
                        let time_offline = instant.elapsed().as_secs();
                        // If instance has been offline for longer than the grace period, terminate the associated device plugin
                        if time_offline >= SHARED_INSTANCE_OFFLINE_GRACE_PERIOD_SECS {
                            remove_instance = true;
                        }
                    }
                }
                if remove_instance {
                    trace!("update_instance_connectivity_status - instance {} has been offline too long ... terminating device plugin", instance);
                    device_plugin_service::terminate_device_plugin_service(
                        &instance,
                        self.instance_map.clone(),
                    )
                    .await
                    .unwrap();
                    k8s::try_delete_instance(
                        (*kube_interface).as_ref(),
                        &instance,
                        self.config.metadata.namespace.as_ref().unwrap(),
                    )
                    .await
                    .unwrap();
                }
            }
        }
        Ok(())
    }
}

pub mod start_discovery {
    use super::super::registration::{
        DiscoveryDetails, DiscoveryHandlerEndpoint, DiscoveryHandlerStatus,
    };
    // Use this `mockall` macro to automate importing a mock type in test mode, or a real type otherwise.
    #[double]
    pub use super::DiscoveryOperator;
    use super::StreamType;
    use akri_shared::k8s;
    use mockall_double::double;
    use std::{sync::Arc, time::Duration};
    use tokio::sync::{broadcast, mpsc};

    /// This is spawned as a task for each Configuration and continues to run
    /// until the Configuration is deleted, at which point, this function is signaled to stop.
    /// It consists of three subtasks:
    /// 1) Initiates discovery on all already registered discovery handlers in the RegisteredDiscoveryHandlerMap
    /// with the same discovery handler name as the Configuration (Configuration.discovery_handler.name).
    /// 2) Listens for new discover handlers to come online for this Configuration and initiates discovery.
    /// 3) Checks whether Offline Instances have exceeded their grace period, in which case it
    /// deletes the Instance.
    pub async fn start_discovery(
        discovery_operator: DiscoveryOperator,
        new_discovery_handler_sender: broadcast::Sender<String>,
        stop_all_discovery_sender: broadcast::Sender<()>,
        finished_all_discovery_sender: &mut mpsc::Sender<()>,
        kube_interface: Arc<Box<dyn k8s::KubeInterface>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        let config = discovery_operator.get_config();
        info!(
            "start_discovery - entered for {} discovery handler",
            config.spec.discovery_handler.name
        );
        let config_name = config.metadata.name.clone();
        let mut tasks = Vec::new();
        let discovery_operator = Arc::new(discovery_operator);

        // Call discover on already registered Discovery Handlers requested by this Configuration's
        let known_dh_discovery_operator = discovery_operator.clone();
        let known_dh_kube_interface = kube_interface.clone();
        tasks.push(tokio::spawn(async move {
            do_discover(known_dh_discovery_operator, known_dh_kube_interface)
                .await
                .unwrap();
        }));

        // Listen for new discovery handlers to call discover on
        let mut stop_all_discovery_receiver = stop_all_discovery_sender.subscribe();
        let mut new_discovery_handler_receiver = new_discovery_handler_sender.subscribe();
        let new_dh_discovery_operator = discovery_operator.clone();
        tasks.push(tokio::spawn(async move {
            listen_for_new_discovery_handlers(
                new_dh_discovery_operator,
                &mut new_discovery_handler_receiver,
                &mut stop_all_discovery_receiver,
            )
            .await
            .unwrap();
        }));

        // Non-local devices are only allowed to be offline for `SHARED_INSTANCE_OFFLINE_GRACE_PERIOD_SECS` minutes before being removed.
        // This task periodically checks if devices have been offline for too long.
        let mut stop_all_discovery_receiver = stop_all_discovery_sender.subscribe();
        let offline_dh_discovery_operator = discovery_operator.clone();
        let offline_dh_kube_interface = kube_interface.clone();
        tasks.push(tokio::spawn(async move {
            loop {
                offline_dh_discovery_operator
                    .delete_offline_instances(offline_dh_kube_interface.clone())
                    .await
                    .unwrap();
                if tokio::time::timeout(
                    Duration::from_secs(30),
                    stop_all_discovery_receiver.recv(),
                )
                .await.is_ok()
                {
                    trace!("start_discovery - received message to stop checking connectivity status for configuration {}", config_name);
                    break;
                }
            }
        }));
        futures::future::try_join_all(tasks).await?;
        finished_all_discovery_sender.send(()).await?;
        Ok(())
    }

    /// Waits to be notified of new discovery handlers. If the discovery handler does discovery for this Configuration,
    /// discovery is kicked off.
    async fn listen_for_new_discovery_handlers(
        discovery_operator: Arc<DiscoveryOperator>,
        new_discovery_handler_receiver: &mut broadcast::Receiver<String>,
        stop_all_discovery_receiver: &mut broadcast::Receiver<()>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        let mut discovery_tasks = Vec::new();
        loop {
            tokio::select! {
                _ = stop_all_discovery_receiver.recv() => {
                    trace!("listen_for_new_discovery_handlers - received message to stop discovery for configuration {}", discovery_operator.get_config().metadata.name);
                    discovery_operator.stop_all_discovery().await;
                    break;
                },
                result = new_discovery_handler_receiver.recv() => {
                    // Check if it is one of this Configuration's discovery handlers
                    if let Ok(discovery_handler_name) = result {
                        if discovery_handler_name == discovery_operator.get_config().spec.discovery_handler.name {
                            trace!("listen_for_new_discovery_handlers - received new registered discovery handler for configuration {}", discovery_operator.get_config().metadata.name);
                            let new_discovery_operator = discovery_operator.clone();
                            discovery_tasks.push(tokio::spawn(async move {
                                do_discover(new_discovery_operator, Arc::new(Box::new(k8s::create_kube_interface()))).await.unwrap();
                            }));
                        }
                    }
                }
            }
        }
        // Wait for all discovery handlers to complete discovery
        futures::future::try_join_all(discovery_tasks).await?;
        Ok(())
    }

    /// A Configuration specifies the name of `DiscoveryHandlers` that should be utilized for discovery.
    /// This tries to establish connection with each `DiscoveryHandler` registered under the requested
    /// `DiscoveryHandler` name and spawns a discovery thread for each connection.
    /// This function also manages the `DiscoveryHandlerStatus` of each `DiscoveryHandler` as follows:
    /// /// `DiscoveryHandlerStatus::Active` if a connection is established via a call to get_stream
    /// /// `DiscoveryHandlerStatus::Waitin`g after a connection has finished due to either being signaled to stop connecting
    /// /// or an error being returned from the discovery handler (that is not a broken pipe)
    /// /// `DiscoveryHandlerStatus::Offline` if a connection cannot be established via a call to get_stream
    /// If a connection cannot be established, continues to try, sleeping between iteration.
    /// Removes the discovery handler from the `RegisteredDiscoveryHandlerMap` if it has been offline for longer than the grace period.
    pub async fn do_discover(
        discovery_operator: Arc<DiscoveryOperator>,
        kube_interface: Arc<Box<dyn k8s::KubeInterface>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        let config = discovery_operator.get_config();
        trace!(
            "do_discover - entered for {} discovery handler",
            config.spec.discovery_handler.name
        );
        // get clone of map
        let mut discovery_handler_map = discovery_operator
            .get_discovery_handler_map()
            .lock()
            .unwrap()
            .clone();
        trace!(
            "do_discover - discovery_handler_map is {:?}",
            discovery_handler_map
        );
        if let Some(discovery_handler_details_map) =
            discovery_handler_map.get_mut(&config.spec.discovery_handler.name)
        {
            for (endpoint, dh_details) in discovery_handler_details_map.clone() {
                trace!(
                    "do_discover - for {} discovery handler at endpoint {:?}",
                    config.spec.discovery_handler.name,
                    endpoint
                );
                // Only use DiscoveryHandler if it doesn't have a client yet
                if dh_details.connectivity_status != DiscoveryHandlerStatus::Active {
                    trace!(
                        "do_discover - {} discovery handler at endpoint {:?} doesn't have client",
                        config.spec.discovery_handler.name,
                        endpoint
                    );
                    let mut stop_discovery_receiver =
                        dh_details.close_discovery_handler_connection.subscribe();
                    loop {
                        tokio::select! {
                            _ = stop_discovery_receiver.recv() => {
                                trace!("do_discover - received message to stop discovery for discovery handler at endpoint {:?} for configuration {}", endpoint, discovery_operator.get_config().metadata.name);
                                break;
                            },
                            _ = do_discover_on_discovery_handler(discovery_operator.clone(), kube_interface.clone(), &endpoint, &dh_details) => {
                                trace!("do_discover - discovery completed for discovery handler at endpoint {:?} for configuration {}", endpoint, discovery_operator.get_config().metadata.name);
                                break;
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Try to connect to discovery handler until connection has been established or grace period has passed
    async fn do_discover_on_discovery_handler<'a>(
        discovery_operator: Arc<DiscoveryOperator>,
        kube_interface: Arc<Box<dyn k8s::KubeInterface>>,
        endpoint: &'a DiscoveryHandlerEndpoint,
        dh_details: &'a DiscoveryDetails,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        loop {
            let deregistered;
            match discovery_operator.get_stream(&endpoint).await {
                Some(stream_type) => {
                    // Since connection was established, be sure that the Discovery Handler is marked as having a client
                    discovery_operator.set_discovery_handler_connectivity_status(
                        &endpoint,
                        DiscoveryHandlerStatus::Active,
                    );
                    match stream_type {
                        StreamType::External(mut stream) => {
                            match discovery_operator
                                .internal_do_discover(
                                    kube_interface.clone(),
                                    &dh_details,
                                    &mut stream,
                                )
                                .await
                            {
                                Ok(_) => {
                                    discovery_operator.set_discovery_handler_connectivity_status(
                                        &endpoint,
                                        DiscoveryHandlerStatus::Waiting,
                                    );
                                    break;
                                }
                                Err(status) => {
                                    if status.message().contains("broken pipe") {
                                        // Mark all associated instances as offline
                                        error!("do_discover_on_discovery_handler - connection with Discovery Handler dropped with status {:?}. Marking all instances offline.", status);
                                        discovery_operator
                                            .update_instance_connectivity_status(
                                                kube_interface.clone(),
                                                std::collections::HashMap::new(),
                                                dh_details.shared,
                                            )
                                            .await?;
                                        deregistered = discovery_operator
                                            .mark_offline_or_deregister_discovery_handler(&endpoint)
                                            .await
                                            .unwrap();
                                    } else {
                                        trace!("do_discover_on_discovery_handler - Discovery Handlers returned error status {}. Marking all instances offline.", status);
                                        // TODO: Possibly mark config as invalid
                                        // Mark all associated instances as offline by declaring no visible instances
                                        discovery_operator
                                            .update_instance_connectivity_status(
                                                kube_interface.clone(),
                                                std::collections::HashMap::new(),
                                                dh_details.shared,
                                            )
                                            .await?;
                                        discovery_operator
                                            .set_discovery_handler_connectivity_status(
                                                &endpoint,
                                                DiscoveryHandlerStatus::Waiting,
                                            );
                                        break;
                                    }
                                }
                            }
                        }
                        StreamType::Embedded(mut stream) => {
                            discovery_operator
                                .internal_do_discover(
                                    kube_interface.clone(),
                                    &dh_details,
                                    &mut stream,
                                )
                                .await
                                .unwrap();
                            discovery_operator.set_discovery_handler_connectivity_status(
                                &endpoint,
                                DiscoveryHandlerStatus::Waiting,
                            );
                            break;
                        }
                    }
                }
                None => {
                    deregistered = discovery_operator
                        .mark_offline_or_deregister_discovery_handler(&endpoint)
                        .await
                        .unwrap();
                }
            }
            if deregistered {
                break;
            } else {
                // Sleep and keep looping until connection established or deregistered due to grace period elapsing
                #[cfg(not(test))]
                tokio::time::delay_for(Duration::from_secs(60)).await;
                #[cfg(test)]
                tokio::time::delay_for(Duration::from_millis(100)).await;
            }
        }
        Ok(())
    }
}

/// Generates an digest of an Instance's id. There should be a unique digest and Instance for each discovered device.
/// This means that the id of non-local devices that could be visible to multiple nodes should always resolve
/// to the same instance name (which is suffixed with this digest).
/// However, local devices' Instances should have unique hashes even if they have the same id.
/// To ensure this, the node's name is added to the id before it is hashed.
pub fn generate_instance_digest(id_to_digest: &str, shared: bool) -> String {
    let env_var_query = ActualEnvVarQuery {};
    inner_generate_instance_digest(id_to_digest, shared, &env_var_query)
}

pub fn inner_generate_instance_digest(
    id_to_digest: &str,
    shared: bool,
    query: &impl EnvVarQuery,
) -> String {
    let mut id_to_digest = id_to_digest.to_string();
    // For local devices, include node hostname in id_to_digest so instances have unique names
    if !shared {
        id_to_digest = format!(
            "{}{}",
            &id_to_digest,
            query.get_env_var("AGENT_NODE_NAME").unwrap()
        );
    }
    let mut digest = String::new();
    let mut hasher = VarBlake2b::new(3).unwrap();
    hasher.update(id_to_digest);
    hasher.finalize_variable(|var| {
        digest = var
            .iter()
            .map(|num| format!("{:02x}", num))
            .collect::<Vec<String>>()
            .join("")
    });
    digest
}

#[cfg(test)]
pub mod tests {
    use super::super::{
        device_plugin_builder::MockDevicePluginBuilderInterface,
        registration::{
            inner_register_embedded_discovery_handlers, DiscoveryDetails, DiscoveryHandlerStatus,
        },
    };
    use super::*;
    use akri_discovery_utils::discovery::mock_discovery_handler;
    use akri_shared::{
        akri::configuration::KubeAkriConfig, k8s::MockKubeInterface, os::env_var::MockEnvVarQuery,
    };
    use mock_instant::{Instant, MockClock};
    use mockall::Sequence;
    use std::time::Duration;
    use tokio::sync::broadcast;

    pub async fn build_instance_map(
        config: &KubeAkriConfig,
        visible_discovery_results: &mut Vec<Device>,
        list_and_watch_message_receivers: &mut Vec<
            broadcast::Receiver<device_plugin_service::ListAndWatchMessageKind>,
        >,
        connectivity_status: InstanceConnectivityStatus,
    ) -> InstanceMap {
        let device1 = Device {
            id: "filter1".to_string(),
            properties: HashMap::new(),
            mounts: Vec::default(),
            device_specs: Vec::default(),
        };
        let device2 = Device {
            id: "filter2".to_string(),
            properties: HashMap::new(),
            mounts: Vec::default(),
            device_specs: Vec::default(),
        };
        let discovery_results = vec![device1, device2];
        *visible_discovery_results = discovery_results.clone();
        generate_instance_map(
            discovery_results,
            list_and_watch_message_receivers,
            connectivity_status,
            &config.metadata.name,
        )
    }

    fn generate_instance_map(
        discovery_results: Vec<Device>,
        list_and_watch_message_receivers: &mut Vec<
            broadcast::Receiver<device_plugin_service::ListAndWatchMessageKind>,
        >,
        connectivity_status: InstanceConnectivityStatus,
        config_name: &str,
    ) -> InstanceMap {
        Arc::new(tokio::sync::Mutex::new(
            discovery_results
                .iter()
                .map(|device| {
                    let (list_and_watch_message_sender, list_and_watch_message_receiver) =
                        broadcast::channel(2);
                    list_and_watch_message_receivers.push(list_and_watch_message_receiver);
                    let instance_name = get_device_instance_name(&device.id, &config_name);
                    (
                        instance_name,
                        InstanceInfo {
                            list_and_watch_message_sender,
                            connectivity_status: connectivity_status.clone(),
                        },
                    )
                })
                .collect(),
        ))
    }

    fn create_mock_discovery_operator(
        discovery_handler_map: RegisteredDiscoveryHandlerMap,
        config: KubeAkriConfig,
        instance_map: InstanceMap,
    ) -> MockDiscoveryOperator {
        let ctx = MockDiscoveryOperator::new_context();
        let discovery_handler_map_clone = discovery_handler_map.clone();
        let config_clone = config.clone();
        let instance_map_clone = instance_map.clone();
        ctx.expect().return_once(move |_, _, _| {
            // let mut discovery_handler_status_seq = Sequence::new();
            let mut mock = MockDiscoveryOperator::default();
            mock.expect_get_discovery_handler_map()
                .returning(move || discovery_handler_map_clone.clone());
            mock.expect_get_config()
                .returning(move || config_clone.clone());
            mock.expect_get_instance_map()
                .returning(move || instance_map_clone.clone());
            mock
        });
        let mock = MockDiscoveryOperator::new(discovery_handler_map, config, instance_map);
        mock
    }

    // Creates a discovery handler with specified properties and adds it to the RegisteredDiscoveryHandlerMap.
    fn add_discovery_handler_to_map(
        dh_name: &str,
        endpoint: &DiscoveryHandlerEndpoint,
        shared: bool,
        registered_dh_map: RegisteredDiscoveryHandlerMap,
    ) {
        let discovery_handler_details =
            create_discovery_handler_details(dh_name, endpoint.clone(), shared);
        // Add discovery handler to registered discovery handler map
        let dh_details_map = match registered_dh_map.lock().unwrap().clone().get_mut(dh_name) {
            Some(dh_details_map) => {
                dh_details_map.insert(endpoint.clone(), discovery_handler_details);
                dh_details_map.clone()
            }
            None => {
                let mut dh_details_map = HashMap::new();
                dh_details_map.insert(endpoint.clone(), discovery_handler_details);
                dh_details_map
            }
        };
        registered_dh_map
            .lock()
            .unwrap()
            .insert(dh_name.to_string(), dh_details_map);
    }

    fn create_discovery_handler_details(
        name: &str,
        endpoint: DiscoveryHandlerEndpoint,
        shared: bool,
    ) -> DiscoveryDetails {
        let (close_discovery_handler_connection, _) = broadcast::channel(2);
        DiscoveryDetails {
            name: name.to_string(),
            endpoint,
            shared,
            close_discovery_handler_connection: close_discovery_handler_connection.clone(),
            connectivity_status: DiscoveryHandlerStatus::Waiting,
        }
    }

    fn setup_test_do_discover() -> (MockDiscoveryOperator, RegisteredDiscoveryHandlerMap) {
        let discovery_handler_map = Arc::new(std::sync::Mutex::new(HashMap::new()));
        add_discovery_handler_to_map(
            "debugEcho",
            &DiscoveryHandlerEndpoint::Uds("socket.sock".to_string()),
            false,
            discovery_handler_map.clone(),
        );

        // Build discovery operator
        let path_to_config = "../test/yaml/config-a.yaml";
        let config_yaml = std::fs::read_to_string(path_to_config).expect("Unable to read file");
        let config: KubeAkriConfig = serde_yaml::from_str(&config_yaml).unwrap();
        let discovery_operator = create_mock_discovery_operator(
            discovery_handler_map.clone(),
            config,
            Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        );
        (discovery_operator, discovery_handler_map)
    }

    #[test]
    fn test_generate_instance_digest() {
        let mut mock_env_var_a = MockEnvVarQuery::new();
        mock_env_var_a
            .expect_get_env_var()
            .returning(|_| Ok("node-a".to_string()));
        let id = "video1";
        let first_unshared_video_digest =
            inner_generate_instance_digest(id, false, &mock_env_var_a);
        let first_shared_video_digest = inner_generate_instance_digest(id, true, &mock_env_var_a);
        let mut mock_env_var_b = MockEnvVarQuery::new();
        mock_env_var_b
            .expect_get_env_var()
            .returning(|_| Ok("node-b".to_string()));
        let second_unshared_video_digest =
            inner_generate_instance_digest(id, false, &mock_env_var_b);
        let second_shared_video_digest = inner_generate_instance_digest(id, true, &mock_env_var_b);
        // unshared instances visible to different nodes should NOT have the same digest
        assert_ne!(first_unshared_video_digest, second_unshared_video_digest);
        // shared instances visible to different nodes should have the same digest
        assert_eq!(first_shared_video_digest, second_shared_video_digest);
    }

    #[tokio::test]
    async fn test_stop_all_discovery() {
        let dh_name = "debugEcho";
        let discovery_handler_map = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let endpoint1 = DiscoveryHandlerEndpoint::Uds("socket.sock".to_string());
        add_discovery_handler_to_map(dh_name, &endpoint1, false, discovery_handler_map.clone());
        let mut close_discovery_handler_connection_receiver1 = discovery_handler_map
            .lock()
            .unwrap()
            .get(dh_name)
            .unwrap()
            .get(&endpoint1)
            .unwrap()
            .close_discovery_handler_connection
            .subscribe();
        let endpoint2 = DiscoveryHandlerEndpoint::Uds("socket2.sock".to_string());
        add_discovery_handler_to_map(dh_name, &endpoint2, false, discovery_handler_map.clone());
        let mut close_discovery_handler_connection_receiver2 = discovery_handler_map
            .lock()
            .unwrap()
            .get(dh_name)
            .unwrap()
            .get(&endpoint2)
            .unwrap()
            .close_discovery_handler_connection
            .subscribe();
        let path_to_config = "../test/yaml/config-a.yaml";
        let config_yaml = std::fs::read_to_string(path_to_config).expect("Unable to read file");
        let config: KubeAkriConfig = serde_yaml::from_str(&config_yaml).unwrap();
        let discovery_operator = Arc::new(DiscoveryOperator::new(
            discovery_handler_map,
            config,
            Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        ));
        tokio::spawn(async move {
            discovery_operator.stop_all_discovery().await;
        });
        assert!(close_discovery_handler_connection_receiver1
            .recv()
            .await
            .is_ok());
        assert!(close_discovery_handler_connection_receiver2
            .recv()
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn test_start_discovery_termination() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (mut mock_discovery_operator, discovery_handler_map) = setup_test_do_discover();
        let (marked_offline_sender, mut marked_offline_receiver) =
            tokio::sync::broadcast::channel(1);
        mock_discovery_operator
            .expect_get_stream()
            .returning(|_| None);
        mock_discovery_operator
            .expect_mark_offline_or_deregister_discovery_handler()
            .withf(move |endpoint: &DiscoveryHandlerEndpoint| {
                endpoint == &DiscoveryHandlerEndpoint::Uds("socket.sock".to_string())
            })
            .returning(move |_| {
                marked_offline_sender.clone().send(()).unwrap();
                Ok(false)
            });
        mock_discovery_operator
            .expect_delete_offline_instances()
            .times(1)
            .returning(move |_| Ok(()));
        let stop_dh_discovery_sender = discovery_handler_map
            .lock()
            .unwrap()
            .get_mut("debugEcho")
            .unwrap()
            .clone()
            .get(&DiscoveryHandlerEndpoint::Uds("socket.sock".to_string()))
            .unwrap()
            .clone()
            .close_discovery_handler_connection
            .clone();
        mock_discovery_operator
            .expect_stop_all_discovery()
            .times(1)
            .returning(move || {
                stop_dh_discovery_sender.clone().send(()).unwrap();
            });
        let (new_dh_sender, _) = broadcast::channel(2);
        let (stop_all_discovery_sender, _) = broadcast::channel(2);
        let (mut finished_discovery_sender, mut finished_discovery_receiver) =
            tokio::sync::mpsc::channel(2);
        let thread_new_dh_sender = new_dh_sender.clone();
        let thread_stop_all_discovery_sender = stop_all_discovery_sender.clone();
        let mock_kube_interface: Arc<Box<dyn k8s::KubeInterface>> =
            Arc::new(Box::new(MockKubeInterface::new()));
        let handle = tokio::spawn(async move {
            start_discovery::start_discovery(
                mock_discovery_operator,
                thread_new_dh_sender,
                thread_stop_all_discovery_sender,
                &mut finished_discovery_sender,
                mock_kube_interface,
            )
            .await
            .unwrap();
        });

        // Wait until do_discovery has gotten to point the DH marked offline
        marked_offline_receiver.recv().await.unwrap();
        stop_all_discovery_sender.send(()).unwrap();
        finished_discovery_receiver.recv().await.unwrap();
        // Make sure that all threads have finished
        handle.await.unwrap();
    }

    // Test that DH is connected to on second try getting stream and
    // that connectivity status is changed from Waiting -> Active -> Waiting again
    // when a successful connection is made and completed.
    #[tokio::test]
    async fn test_do_discover_completed_internal_connection() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (mut mock_discovery_operator, _) = setup_test_do_discover();
        let mut get_stream_seq = Sequence::new();
        // First time cannot get stream and is marked offline
        mock_discovery_operator
            .expect_get_stream()
            .times(1)
            .returning(|_| None)
            .in_sequence(&mut get_stream_seq);
        mock_discovery_operator
            .expect_mark_offline_or_deregister_discovery_handler()
            .withf(move |endpoint: &DiscoveryHandlerEndpoint| {
                endpoint == &DiscoveryHandlerEndpoint::Uds("socket.sock".to_string())
            })
            .times(1)
            .returning(|_| Ok(false));
        // Second time successfully get stream
        let (_, rx) = mpsc::channel(2);
        let stream_type = Some(StreamType::Embedded(rx));
        mock_discovery_operator
            .expect_get_stream()
            .times(1)
            .return_once(move |_| stream_type)
            .in_sequence(&mut get_stream_seq);
        // Make sure discovery handler is marked as Active
        let mut discovery_handler_status_seq = Sequence::new();
        mock_discovery_operator
            .expect_set_discovery_handler_connectivity_status()
            .withf(
                move |endpoint: &DiscoveryHandlerEndpoint,
                      connectivity_status: &DiscoveryHandlerStatus| {
                    endpoint == &DiscoveryHandlerEndpoint::Uds("socket.sock".to_string())
                        && connectivity_status == &DiscoveryHandlerStatus::Active
                },
            )
            .times(1)
            .returning(|_, _| ())
            .in_sequence(&mut discovery_handler_status_seq);
        // Discovery should be initiated
        mock_discovery_operator
            .expect_internal_do_discover()
            .times(1)
            .returning(|_, _, _| Ok(()));
        // Make sure after discovery is complete that the DH is marked Online again
        mock_discovery_operator
            .expect_set_discovery_handler_connectivity_status()
            .withf(
                move |endpoint: &DiscoveryHandlerEndpoint,
                      connectivity_status: &DiscoveryHandlerStatus| {
                    endpoint == &DiscoveryHandlerEndpoint::Uds("socket.sock".to_string())
                        && connectivity_status == &DiscoveryHandlerStatus::Waiting
                },
            )
            .times(1)
            .returning(|_, _| ())
            .in_sequence(&mut discovery_handler_status_seq);
        let mock_kube_interface: Arc<Box<dyn k8s::KubeInterface>> =
            Arc::new(Box::new(MockKubeInterface::new()));
        start_discovery::do_discover(Arc::new(mock_discovery_operator), mock_kube_interface)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_handle_discovery_results() {
        let _ = env_logger::builder().is_test(true).try_init();
        // Set node name for generating instance id
        std::env::set_var("AGENT_NODE_NAME", "node-a");
        let mock_kube_interface: Arc<Box<dyn k8s::KubeInterface>> =
            Arc::new(Box::new(MockKubeInterface::new()));
        let discovery_handler_map: RegisteredDiscoveryHandlerMap =
            Arc::new(std::sync::Mutex::new(HashMap::new()));
        let path_to_config = "../test/yaml/config-a.yaml";
        let config_yaml = std::fs::read_to_string(path_to_config).expect("Unable to read file");
        let config: KubeAkriConfig = serde_yaml::from_str(&config_yaml).unwrap();
        let config_name = config.metadata.name.clone();
        INSTANCE_COUNT_METRIC
            .with_label_values(&[&config_name, "true"])
            .set(0);
        let device1 = Device {
            id: "device1".to_string(),
            properties: HashMap::new(),
            mounts: Vec::default(),
            device_specs: Vec::default(),
        };
        let device2 = Device {
            id: "device2".to_string(),
            properties: HashMap::new(),
            mounts: Vec::default(),
            device_specs: Vec::default(),
        };
        let discovery_results: Vec<Device> = vec![device1, device2];
        let discovery_operator = Arc::new(DiscoveryOperator::new(
            discovery_handler_map,
            config,
            Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        ));
        let mut mock_device_plugin_builder = MockDevicePluginBuilderInterface::new();
        mock_device_plugin_builder
            .expect_build_device_plugin()
            .times(2)
            .returning(move |_, _, _, _, _| Ok(()));
        discovery_operator
            .handle_discovery_results(
                mock_kube_interface,
                discovery_results,
                true,
                Box::new(mock_device_plugin_builder),
            )
            .await
            .unwrap();

        assert_eq!(
            INSTANCE_COUNT_METRIC
                .with_label_values(&[&config_name, "true"])
                .get(),
            2
        );
    }

    // Checks either that InstanceConnectivityStatus changed to expected value until success or exceeded tries
    // or that all instances have been deleted from map.
    // Sleep between tries to give update_instance_connectivity_status the chance chance to grab mutex InstanceMap.
    async fn check_status_or_empty_loop(
        status: InstanceConnectivityStatus,
        equality: bool,
        instance_map: InstanceMap,
        check_empty: bool,
    ) {
        let mut keep_looping = false;
        let mut map_is_empty = false;
        let tries: i8 = 5;
        for _x in 0..tries {
            println!("try number {}", _x);
            keep_looping = false;
            tokio::time::delay_for(Duration::from_millis(100)).await;
            let unwrapped_instance_map = instance_map.lock().await.clone();
            if check_empty && unwrapped_instance_map.is_empty() {
                map_is_empty = true;
                break;
            }
            for (_, instance_info) in unwrapped_instance_map {
                if instance_info.connectivity_status != status && equality {
                    keep_looping = true;
                }
                if instance_info.connectivity_status == status && !equality {
                    keep_looping = true;
                }
            }
            if !keep_looping {
                break;
            }
        }
        if keep_looping {
            panic!(
                "failed to assert that all instances had status equal T/F: [{}] to status [{:?}]",
                equality, status
            );
        }
        if check_empty && !map_is_empty {
            panic!("instances were not cleared from map");
        }
    }

    #[tokio::test]
    async fn test_delete_offline_instances() {
        let _ = env_logger::builder().is_test(true).try_init();
        let path_to_config = "../test/yaml/config-a.yaml";
        let config_yaml = std::fs::read_to_string(path_to_config).expect("Unable to read file");
        let config: KubeAkriConfig = serde_yaml::from_str(&config_yaml).unwrap();
        let mut list_and_watch_message_receivers = Vec::new();
        let discovery_handler_map: RegisteredDiscoveryHandlerMap =
            Arc::new(std::sync::Mutex::new(HashMap::new()));
        let mut visible_discovery_results = Vec::new();

        // Assert no action (to delete instances by mock kube interface) is taken for all online instances
        let instance_map: InstanceMap = build_instance_map(
            &config,
            &mut visible_discovery_results,
            &mut list_and_watch_message_receivers,
            InstanceConnectivityStatus::Online,
        )
        .await;
        let mock = MockKubeInterface::new();
        let discovery_operator = Arc::new(DiscoveryOperator::new(
            discovery_handler_map.clone(),
            config.clone(),
            instance_map,
        ));
        discovery_operator
            .delete_offline_instances(Arc::new(Box::new(mock)))
            .await
            .unwrap();

        // Assert no action (to delete instances by mock kube interface) is taken for instances offline for less than grace period
        let mock_now = Instant::now();
        MockClock::advance(Duration::from_secs(30));
        let instance_map: InstanceMap = build_instance_map(
            &config,
            &mut visible_discovery_results,
            &mut list_and_watch_message_receivers,
            InstanceConnectivityStatus::Offline(mock_now),
        )
        .await;
        let mock = MockKubeInterface::new();
        let discovery_operator = Arc::new(DiscoveryOperator::new(
            discovery_handler_map.clone(),
            config.clone(),
            instance_map,
        ));
        discovery_operator
            .delete_offline_instances(Arc::new(Box::new(mock)))
            .await
            .unwrap();

        // Assert that all instances that have been offline for more than 5 minutes are deleted
        let mock_now = Instant::now();
        MockClock::advance(Duration::from_secs(301));
        let instance_map: InstanceMap = build_instance_map(
            &config,
            &mut visible_discovery_results,
            &mut list_and_watch_message_receivers,
            InstanceConnectivityStatus::Offline(mock_now),
        )
        .await;
        let mut mock = MockKubeInterface::new();
        mock.expect_delete_instance()
            .times(2)
            .returning(move |_, _| Ok(()));
        let discovery_operator = Arc::new(DiscoveryOperator::new(
            discovery_handler_map.clone(),
            config.clone(),
            instance_map.clone(),
        ));
        discovery_operator
            .delete_offline_instances(Arc::new(Box::new(mock)))
            .await
            .unwrap();
        // Make sure all instances are deleted from map. Note, first 3 arguments are ignored.
        check_status_or_empty_loop(InstanceConnectivityStatus::Online, true, instance_map, true)
            .await;
    }

    // 1: InstanceConnectivityStatus of all instances that go offline is changed from Online to Offline
    // 2: InstanceConnectivityStatus of shared instances that come back online in under 5 minutes is changed from Offline to Online
    // 3: InstanceConnectivityStatus of unshared instances that come back online before next periodic discovery is changed from Offline to Online
    #[tokio::test(core_threads = 2)]
    async fn test_update_instance_connectivity_status_factory() {
        let _ = env_logger::builder().is_test(true).try_init();
        let path_to_config = "../test/yaml/config-a.yaml";
        let config_yaml = std::fs::read_to_string(path_to_config).expect("Unable to read file");
        let config: KubeAkriConfig = serde_yaml::from_str(&config_yaml).unwrap();
        let config_name = config.metadata.name.clone();
        let mut list_and_watch_message_receivers = Vec::new();
        let mut visible_discovery_results = Vec::new();
        let discovery_handler_map: RegisteredDiscoveryHandlerMap =
            Arc::new(std::sync::Mutex::new(HashMap::new()));
        let discovery_handler_map_clone = discovery_handler_map.clone();
        // set environment variable to set whether debug echo instances are shared
        let mut mock_env_var_shared = MockEnvVarQuery::new();
        mock_env_var_shared
            .expect_get_env_var()
            .returning(|_| Ok("false".to_string()));
        inner_register_embedded_discovery_handlers(
            discovery_handler_map_clone,
            &mock_env_var_shared,
        )
        .unwrap();

        //
        // 1: Assert that InstanceConnectivityStatus of non local instances that are no longer visible is changed to Offline
        //
        let instance_map: InstanceMap = build_instance_map(
            &config,
            &mut visible_discovery_results,
            &mut list_and_watch_message_receivers,
            InstanceConnectivityStatus::Online,
        )
        .await;
        let shared = true;
        run_update_instance_connectivity_status(
            config.clone(),
            HashMap::new(),
            shared,
            instance_map.clone(),
            discovery_handler_map.clone(),
            MockKubeInterface::new(),
        )
        .await;

        // Check that no instances are still online
        check_status_or_empty_loop(
            InstanceConnectivityStatus::Online,
            false,
            instance_map,
            false,
        )
        .await;

        //
        // 2: Assert that InstanceConnectivityStatus of shared instances that come back online in <5 mins is changed to Online
        //
        let mock_now = Instant::now();
        MockClock::advance(Duration::from_secs(30));
        let instance_map: InstanceMap = build_instance_map(
            &config,
            &mut visible_discovery_results,
            &mut list_and_watch_message_receivers,
            InstanceConnectivityStatus::Offline(mock_now),
        )
        .await;
        let currently_visible_instances: HashMap<String, Device> = visible_discovery_results
            .iter()
            .map(|device| {
                let instance_name = get_device_instance_name(&device.id, &config_name);
                (instance_name, device.clone())
            })
            .collect();
        let shared = true;
        run_update_instance_connectivity_status(
            config.clone(),
            currently_visible_instances.clone(),
            shared,
            instance_map.clone(),
            discovery_handler_map.clone(),
            MockKubeInterface::new(),
        )
        .await;

        // Check that all instances marked online
        check_status_or_empty_loop(
            InstanceConnectivityStatus::Online,
            true,
            instance_map,
            false,
        )
        .await;

        //
        // 3: Assert that shared instances that are offline for more than 5 minutes are removed from the instance map
        //
        let mock_now = Instant::now();
        MockClock::advance(Duration::from_secs(301));
        let instance_map: InstanceMap = build_instance_map(
            &config,
            &mut visible_discovery_results,
            &mut list_and_watch_message_receivers,
            InstanceConnectivityStatus::Offline(mock_now),
        )
        .await;
        let mut mock = MockKubeInterface::new();
        mock.expect_delete_instance()
            .times(2)
            .returning(move |_, _| Ok(()));
        let shared = true;
        run_update_instance_connectivity_status(
            config.clone(),
            HashMap::new(),
            shared,
            instance_map.clone(),
            discovery_handler_map.clone(),
            mock,
        )
        .await;
        // Make sure all instances are deleted from map. Note, first 3 arguments are ignored.
        check_status_or_empty_loop(InstanceConnectivityStatus::Online, true, instance_map, true)
            .await;

        //
        // 4: Assert that local devices that go offline are removed from the instance map
        //
        let mut mock = MockKubeInterface::new();
        mock.expect_delete_instance()
            .times(2)
            .returning(move |_, _| Ok(()));

        let instance_map: InstanceMap = build_instance_map(
            &config,
            &mut visible_discovery_results,
            &mut list_and_watch_message_receivers,
            InstanceConnectivityStatus::Online,
        )
        .await;
        let shared = false;
        run_update_instance_connectivity_status(
            config,
            HashMap::new(),
            shared,
            instance_map.clone(),
            discovery_handler_map.clone(),
            mock,
        )
        .await;
        // Make sure all instances are deleted from map. Note, first 3 arguments are ignored.
        check_status_or_empty_loop(InstanceConnectivityStatus::Online, true, instance_map, true)
            .await;
    }

    async fn run_update_instance_connectivity_status(
        config: KubeAkriConfig,
        currently_visible_instances: HashMap<String, Device>,
        shared: bool,
        instance_map: InstanceMap,
        discovery_handler_map: RegisteredDiscoveryHandlerMap,
        mock: MockKubeInterface,
    ) {
        let discovery_operator = Arc::new(DiscoveryOperator::new(
            discovery_handler_map,
            config,
            instance_map.clone(),
        ));
        discovery_operator
            .update_instance_connectivity_status(
                Arc::new(Box::new(mock)),
                currently_visible_instances,
                shared,
            )
            .await
            .unwrap();
    }

    fn setup_non_mocked_dh(
        dh_name: &str,
        endpoint: &DiscoveryHandlerEndpoint,
    ) -> DiscoveryOperator {
        let path_to_config = "../test/yaml/config-a.yaml";
        let config_yaml = std::fs::read_to_string(path_to_config).expect("Unable to read file");
        let config: KubeAkriConfig = serde_yaml::from_str(&config_yaml).unwrap();
        let discovery_handler_map = Arc::new(std::sync::Mutex::new(HashMap::new()));
        add_discovery_handler_to_map(dh_name, endpoint, false, discovery_handler_map.clone());
        DiscoveryOperator::new(
            discovery_handler_map,
            config,
            Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        )
    }

    #[tokio::test]
    async fn test_set_discovery_handler_connectivity_status() {
        let _ = env_logger::builder().is_test(true).try_init();
        let discovery_handler_name = "debugEcho";
        let endpoint = DiscoveryHandlerEndpoint::Uds("socket.sock".to_string());
        let discovery_operator = setup_non_mocked_dh(discovery_handler_name, &endpoint);
        // Test that an online discovery handler is marked Active
        discovery_operator
            .set_discovery_handler_connectivity_status(&endpoint, DiscoveryHandlerStatus::Active);
        assert_eq!(
            discovery_operator
                .discovery_handler_map
                .lock()
                .unwrap()
                .get_mut(discovery_handler_name)
                .unwrap()
                .clone()
                .get(&endpoint)
                .unwrap()
                .clone()
                .connectivity_status,
            DiscoveryHandlerStatus::Active
        );
    }

    #[tokio::test]
    async fn test_mark_offline_or_deregister_discovery_handler() {
        let _ = env_logger::builder().is_test(true).try_init();
        let discovery_handler_name = "debugEcho";
        let endpoint = DiscoveryHandlerEndpoint::Uds("socket.sock".to_string());
        let discovery_operator = setup_non_mocked_dh(discovery_handler_name, &endpoint);
        // Test that an online discovery handler is marked offline
        assert_eq!(
            discovery_operator
                .mark_offline_or_deregister_discovery_handler(&endpoint)
                .await
                .unwrap(),
            false
        );
        if let DiscoveryHandlerStatus::Offline(_) = discovery_operator
            .discovery_handler_map
            .lock()
            .unwrap()
            .get_mut(discovery_handler_name)
            .unwrap()
            .clone()
            .get(&endpoint)
            .unwrap()
            .clone()
            .connectivity_status
        {
            // expected
        } else {
            panic!("DiscoveryHandlerStatus should be changed to offline");
        }
        // Test that an offline discovery handler IS NOT deregistered if the time has not passed
        assert_eq!(
            discovery_operator
                .mark_offline_or_deregister_discovery_handler(&endpoint)
                .await
                .unwrap(),
            false
        );

        // Test that an offline discovery handler IS deregistered if the time has passed
        let mock_now = Instant::now();
        MockClock::advance(Duration::from_secs(301));
        discovery_operator
            .discovery_handler_map
            .lock()
            .unwrap()
            .get_mut(discovery_handler_name)
            .unwrap()
            .get_mut(&endpoint)
            .unwrap()
            .connectivity_status = DiscoveryHandlerStatus::Offline(mock_now);
        assert_eq!(
            discovery_operator
                .mark_offline_or_deregister_discovery_handler(&endpoint)
                .await
                .unwrap(),
            true
        );
    }

    #[tokio::test]
    async fn test_get_stream_embedded() {
        let _ = env_logger::builder().is_test(true).try_init();
        std::env::set_var(super::super::constants::ENABLE_DEBUG_ECHO_LABEL, "yes");
        let path_to_config = "../test/yaml/config-a.yaml";
        let config_yaml = std::fs::read_to_string(path_to_config).expect("Unable to read file");
        let config: KubeAkriConfig = serde_yaml::from_str(&config_yaml).unwrap();
        let discovery_handler_map = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let endpoint = DiscoveryHandlerEndpoint::Embedded;
        let dh_name = akri_debug_echo::DISCOVERY_HANDLER_NAME.to_string();
        add_discovery_handler_to_map(&dh_name, &endpoint, false, discovery_handler_map.clone());
        let discovery_operator = DiscoveryOperator::new(
            discovery_handler_map,
            config,
            Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        );
        // test embedded debugEcho socket
        if let Some(StreamType::Embedded(_)) = discovery_operator
            .get_stream(&DiscoveryHandlerEndpoint::Embedded)
            .await
        {
            // expected
        } else {
            panic!("expected internal stream");
        }
    }

    async fn setup_and_run_mock_discovery_handler(
        endpoint: &str,
        endpoint_dir: &str,
        dh_endpoint: &DiscoveryHandlerEndpoint,
        return_error: bool,
    ) -> DiscoveryOperator {
        let discovery_operator = setup_non_mocked_dh("mockName", dh_endpoint);
        // Start mock DH, specifying that it should successfully run
        let _dh_server_thread_handle = mock_discovery_handler::run_mock_discovery_handler(
            endpoint_dir,
            endpoint,
            return_error,
        )
        .await;
        // Make sure registration server has started
        akri_shared::uds::unix_stream::try_connect(&endpoint)
            .await
            .unwrap();
        discovery_operator
    }

    #[tokio::test]
    async fn test_get_stream_no_dh() {
        let (_, endpoint) =
            mock_discovery_handler::get_mock_discovery_handler_dir_and_endpoint("mock.sock");
        let dh_endpoint = DiscoveryHandlerEndpoint::Uds(endpoint.to_string());
        let discovery_operator = setup_non_mocked_dh("mock", &dh_endpoint);
        // Should not be able to get stream if DH is not running
        assert!(discovery_operator.get_stream(&dh_endpoint).await.is_none());
    }

    #[tokio::test]
    async fn test_get_stream_error() {
        // Start mock DH, specifying that it should return an error
        let return_error = true;
        let (endpoint_dir, endpoint) =
            mock_discovery_handler::get_mock_discovery_handler_dir_and_endpoint("mock.sock");
        let dh_endpoint = DiscoveryHandlerEndpoint::Uds(endpoint.to_string());
        let discovery_operator = setup_and_run_mock_discovery_handler(
            &endpoint,
            &endpoint_dir,
            &dh_endpoint,
            return_error,
        )
        .await;
        // Assert that get_stream returns none if the DH returns error
        assert!(discovery_operator.get_stream(&dh_endpoint).await.is_none());
    }

    #[tokio::test]
    async fn test_get_stream_external_success() {
        // Start mock DH, specifying that it should NOT return an error
        let return_error = false;
        let (endpoint_dir, endpoint) =
            mock_discovery_handler::get_mock_discovery_handler_dir_and_endpoint("mock.sock");
        let dh_endpoint = DiscoveryHandlerEndpoint::Uds(endpoint.to_string());
        let discovery_operator = setup_and_run_mock_discovery_handler(
            &endpoint,
            &endpoint_dir,
            &dh_endpoint,
            return_error,
        )
        .await;
        if let Some(StreamType::External(mut receiver)) =
            discovery_operator.get_stream(&dh_endpoint).await
        {
            // MockDiscoveryHandler returns an empty array of devices
            assert_eq!(
                receiver.get_message().await.unwrap().unwrap().devices.len(),
                0
            );
        } else {
            panic!("expected external stream");
        }
    }
}
