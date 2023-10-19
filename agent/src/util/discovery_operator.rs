#[cfg(any(test, feature = "agent-full"))]
use super::embedded_discovery_handlers::get_discovery_handler;
use super::metrics::INSTANCE_COUNT_METRIC;
use super::{
    config_action::ConfigId,
    constants::SHARED_INSTANCE_OFFLINE_GRACE_PERIOD_SECS,
    device_plugin_builder::{DevicePluginBuilder, DevicePluginBuilderInterface},
    device_plugin_service,
    device_plugin_service::{
        get_device_instance_name, DevicePluginContext, InstanceConnectivityStatus, InstanceInfo,
    },
    registration::{DiscoveryDetails, DiscoveryHandlerEndpoint, RegisteredDiscoveryHandlerMap},
    streaming_extension::StreamingExt,
};
use akri_discovery_utils::discovery::v0::{
    discovery_handler_client::DiscoveryHandlerClient, ByteData, Device, DiscoverRequest,
    DiscoverResponse,
};
use akri_shared::{
    akri::{
        configuration::{
            Configuration, DiscoveryProperty, DiscoveryPropertyKeySelector, DiscoveryPropertySource,
        },
        retry::MAX_INSTANCE_UPDATE_TRIES,
    },
    k8s,
};
use blake2::{
    digest::{Update, VariableOutput},
    VarBlake2b,
};
use k8s_openapi::api::core::v1::{ConfigMap, Secret};
use kube::api::Api;
use log::{error, trace};
#[cfg(test)]
use mock_instant::Instant;
#[cfg(test)]
use mockall::{automock, predicate::*};
use std::io::{Error, ErrorKind};
#[cfg(not(test))]
use std::time::Instant;
use std::{collections::HashMap, convert::TryFrom, sync::Arc};
use tokio::sync::RwLock;
use tonic::transport::{Endpoint, Uri};

/// StreamType provides a wrapper around the two different types of streams returned from embedded
/// or embedded discovery handlers and ones running externally.
pub enum StreamType {
    #[cfg(any(test, feature = "agent-full"))]
    Embedded(tokio::sync::mpsc::Receiver<std::result::Result<DiscoverResponse, tonic::Status>>),
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
    config: Configuration,
    /// Akri Instances discovered by this `DiscoveryOperator`
    device_plugin_context: Arc<RwLock<DevicePluginContext>>,
    /// Timestamp of DiscoveryOperator is created when config is created or updated
    config_timestamp: Instant,
}

#[cfg_attr(test, automock)]
impl DiscoveryOperator {
    pub fn new(
        discovery_handler_map: RegisteredDiscoveryHandlerMap,
        config: Configuration,
        device_plugin_context: Arc<RwLock<DevicePluginContext>>,
    ) -> Self {
        DiscoveryOperator {
            discovery_handler_map,
            config,
            device_plugin_context,
            config_timestamp: Instant::now(),
        }
    }
    fn get_config_id(&self) -> ConfigId {
        (
            self.config.metadata.namespace.clone().unwrap(),
            self.config.metadata.name.clone().unwrap(),
        )
    }

    /// Returns discovery_handler_map field. Allows the struct to be mocked.
    #[allow(dead_code)]
    pub fn get_discovery_handler_map(&self) -> RegisteredDiscoveryHandlerMap {
        self.discovery_handler_map.clone()
    }
    /// Returns config field. Allows the struct to be mocked.
    #[allow(dead_code)]
    pub fn get_config(&self) -> Configuration {
        self.config.clone()
    }
    /// Returns device_plugin_context field. Allows the struct to be mocked.
    #[allow(dead_code)]
    pub fn get_device_plugin_context(&self) -> Arc<RwLock<DevicePluginContext>> {
        self.device_plugin_context.clone()
    }
    /// Returns config_timestamp field. Allows the struct to be mocked.
    #[allow(dead_code)]
    pub fn get_config_timestamp(&self) -> Instant {
        self.config_timestamp
    }
    #[allow(dead_code)]
    pub async fn stop_all_discovery(&self) {
        let mut discovery_handler_map = self.discovery_handler_map.lock().unwrap().clone();
        if let Some(discovery_handler_details_map) =
            discovery_handler_map.get_mut(&self.config.spec.discovery_handler.name)
        {
            for (endpoint, dh_details) in discovery_handler_details_map.clone() {
                // Send with the config_id so we stop discovery for this Configuration only.
                match dh_details.close_discovery_handler_connection.send(Some(self.get_config_id())) {
                    Ok(_) => trace!("stop_all_discovery - discovery client for {} discovery handler at endpoint {:?} told to stop", self.config.spec.discovery_handler.name, endpoint),
                    Err(e) => error!("stop_all_discovery - discovery client {} discovery handler at endpoint {:?} could not receive stop message with error {:?}", self.config.spec.discovery_handler.name, endpoint, e)
                }
            }
        }
    }

    /// Calls discover on the Discovery Handler at the given endpoint and returns the connection stream.
    pub async fn get_stream<'a>(
        &'a self,
        kube_interface: Arc<dyn k8s::KubeInterface>,
        endpoint: &'a DiscoveryHandlerEndpoint,
    ) -> Option<StreamType> {
        let discovery_properties = match self
            .get_discovery_properties(
                kube_interface.clone(),
                &self.config.spec.discovery_handler.discovery_properties,
            )
            .await
        {
            Ok(data) => data,
            Err(e) => {
                error!(
                    "get_stream - fail to retrieve discovery properties for Configuration {:?}, error {:?}",
                    self.config.metadata.name, e
                );
                return None;
            }
        };
        let discover_request = tonic::Request::new(DiscoverRequest {
            discovery_details: self.config.spec.discovery_handler.discovery_details.clone(),
            discovery_properties,
        });
        trace!("get_stream - endpoint is {:?}", endpoint);
        match endpoint {
            #[cfg(any(test, feature = "agent-full"))]
            DiscoveryHandlerEndpoint::Embedded => {
                match get_discovery_handler(&self.config.spec.discovery_handler) {
                    Ok(discovery_handler) => {
                        trace!(
                            "get_stream - using embedded {} discovery handler",
                            self.config.spec.discovery_handler.name
                        );
                        match discovery_handler.discover(discover_request).await {
                            Ok(device_update_receiver) => Some(StreamType::Embedded(
                                // `discover` returns `Result<tonic::Response<Self::DiscoverStream>, tonic::Status>`
                                // Get the `Receiver` from the `DiscoverStream` wrapper
                                device_update_receiver.into_inner().into_inner(),
                            )),
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
                // Some servers will check the uri content so the uri needs to
                // be in valid format even it's not used, the scheme part is used
                // to specific what scheme to use, such as http or https
                match Endpoint::try_from("http://[::1]:50051")
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
    /// Runs until notified to stop discovery.
    #[allow(dead_code)]
    pub async fn internal_do_discover<'a>(
        &'a self,
        kube_interface: Arc<dyn k8s::KubeInterface>,
        dh_details: &'a DiscoveryDetails,
        stream: &'a mut dyn StreamingExt,
        node_name: String,
    ) -> anyhow::Result<()> {
        // clone objects for thread
        let discovery_operator = Arc::new(self.clone());
        let stop_discovery_receiver: &mut tokio::sync::broadcast::Receiver<Option<ConfigId>> =
            &mut dh_details.close_discovery_handler_connection.subscribe();
        loop {
            // Wait for either new discovery results or a message to stop discovery
            tokio::select! {
                result = stop_discovery_receiver.recv() => {
                    // Stop is triggered if the current config_id (to only stop this task) or None (to stop all tasks of this handler) is sent.
                    if let Ok(Some(config_id)) = result {
                        if config_id != self.get_config_id() {
                            trace!("internal_do_discover - received message to stop discovery for another configuration, ignoring it.");
                            continue;
                        }
                    }
                    trace!(
                        "internal_do_discover({}::{}) - received message to stop discovery for endpoint {:?} serving protocol {}",
                        self.config.metadata.namespace.as_ref().unwrap(),
                        self.config.metadata.name.as_ref().unwrap(),
                        dh_details.endpoint,
                        discovery_operator.get_config().spec.discovery_handler.name,
                    );
                    break;
                },
                result = stream.get_message() => {
                    let response = result?.ok_or_else(|| anyhow::anyhow!("Received response type None. Should not happen."))?;
                    trace!("internal_do_discover - got discovery results {:?}", response.devices);
                    self.handle_discovery_results(
                        kube_interface.clone(),
                        response.devices,
                        dh_details.shared,
                        Box::new(DevicePluginBuilder{}),
                        node_name.clone(),
                    )
                    .await?;
                }
            }
        }

        Ok(())
    }

    /// Checks if any of this DiscoveryOperator's Configuration's Instances have been offline for too long.
    /// If a non-local device has not come back online before `SHARED_INSTANCE_OFFLINE_GRACE_PERIOD_SECS`,
    /// the associated Device Plugin and Instance are terminated and deleted, respectively.
    pub async fn delete_offline_instances(
        &self,
        kube_interface: Arc<dyn k8s::KubeInterface>,
        node_name: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        trace!(
            "delete_offline_instances - entered for configuration {:?}",
            self.config.metadata.name
        );
        let kube_interface_clone = kube_interface.clone();
        let instance_map = self.device_plugin_context.write().await.clone().instances;
        for (instance, instance_info) in instance_map {
            if let InstanceConnectivityStatus::Offline(instant) = instance_info.connectivity_status
            {
                let time_offline = instant.elapsed().as_secs();
                // If instance has been offline for longer than the grace period or it is unshared, terminate the associated device plugin
                // TODO: make grace period configurable
                if time_offline >= SHARED_INSTANCE_OFFLINE_GRACE_PERIOD_SECS {
                    trace!("delete_offline_instances - instance {} has been offline too long ... terminating device plugin", instance);
                    device_plugin_service::terminate_device_plugin_service(
                        &instance,
                        self.device_plugin_context.clone(),
                    )
                    .await
                    .unwrap();
                    try_delete_instance(
                        kube_interface_clone.as_ref(),
                        &instance,
                        self.config.metadata.namespace.as_ref().unwrap(),
                        node_name.clone(),
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
        kube_interface: Arc<dyn k8s::KubeInterface>,
        discovery_results: Vec<Device>,
        shared: bool,
        device_plugin_builder: Box<dyn DevicePluginBuilderInterface>,
        node_name: String,
    ) -> anyhow::Result<()> {
        let config_name = self.config.metadata.name.clone().unwrap();
        trace!(
            "handle_discovery_results - for config {} with discovery results {:?}",
            config_name,
            discovery_results
        );
        let currently_visible_instances: HashMap<String, Device> = discovery_results
            .iter()
            .map(|discovery_result| {
                let id = generate_instance_digest(&discovery_result.id, shared, &node_name);
                let instance_name = get_device_instance_name(&id, &config_name);
                (instance_name, discovery_result.clone())
            })
            .collect();
        INSTANCE_COUNT_METRIC
            .with_label_values(&[&config_name, &shared.to_string()])
            .set(currently_visible_instances.len() as i64);
        // Update the connectivity status of instances and return list of visible instances that don't have Instance CRs
        let device_plugin_context = self.device_plugin_context.read().await.clone();
        // Find all visible instances that do not have Instance CRDs yet
        let new_discovery_results: Vec<Device> = currently_visible_instances
            .iter()
            .filter(|(name, _)| !device_plugin_context.instances.contains_key(*name))
            .map(|(_, p)| p.clone())
            .collect();
        self.update_instance_connectivity_status(
            kube_interface,
            currently_visible_instances,
            shared,
            node_name.clone(),
        )
        .await?;

        // If there are newly visible instances associated with a Config, make a device plugin and Instance CR for them
        if !new_discovery_results.is_empty() {
            for discovery_result in new_discovery_results {
                let id = generate_instance_digest(&discovery_result.id, shared, &node_name);
                let instance_name = get_device_instance_name(&id, &config_name);
                trace!(
                    "handle_discovery_results - new instance {} came online",
                    instance_name
                );
                let device_plugin_context = self.device_plugin_context.clone();
                if let Err(e) = device_plugin_builder
                    .build_device_plugin(
                        id,
                        &self.config,
                        shared,
                        device_plugin_context,
                        discovery_result.clone(),
                        node_name.clone(),
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
        kube_interface: Arc<dyn k8s::KubeInterface>,
        currently_visible_instances: HashMap<String, Device>,
        shared: bool,
        node_name: String,
    ) -> anyhow::Result<()> {
        let instance_map = self.device_plugin_context.read().await.clone().instances;
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
                    let device = currently_visible_instances.get(&instance).unwrap();
                    let updated_instance_info = InstanceInfo {
                        connectivity_status: InstanceConnectivityStatus::Online,
                        list_and_watch_message_sender: list_and_watch_message_sender.clone(),
                        instance_id: instance_info.instance_id.clone(),
                        device: device.clone(),
                    };
                    self.device_plugin_context
                        .write()
                        .await
                        .instances
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
                                instance_id: instance_info.instance_id.clone(),
                                device: instance_info.device.clone(),
                            };
                            self.device_plugin_context
                                .write()
                                .await
                                .instances
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
                        self.device_plugin_context.clone(),
                    )
                    .await
                    .unwrap();
                    try_delete_instance(
                        kube_interface.as_ref(),
                        &instance,
                        self.config.metadata.namespace.as_ref().unwrap(),
                        node_name.clone(),
                    )
                    .await
                    .unwrap();
                }
            }
        }
        Ok(())
    }

    async fn get_discovery_properties(
        &self,
        kube_interface: Arc<dyn k8s::KubeInterface>,
        properties: &Option<Vec<DiscoveryProperty>>,
    ) -> anyhow::Result<HashMap<String, ByteData>> {
        match properties {
            None => Ok(HashMap::new()),
            Some(properties) => {
                let mut tmp_properties = HashMap::new();
                for p in properties {
                    match self.get_discovery_property(kube_interface.clone(), p).await {
                        Ok(tmp_p) => {
                            if let Some((k, v)) = tmp_p {
                                tmp_properties.insert(k, v.clone());
                            }
                        }
                        Err(e) => return Err(e),
                    }
                }
                Ok(tmp_properties)
            }
        }
    }

    async fn get_discovery_property(
        &self,
        kube_interface: Arc<dyn k8s::KubeInterface>,
        property: &DiscoveryProperty,
    ) -> anyhow::Result<Option<(String, ByteData)>> {
        let value;
        if let Some(v) = &property.value {
            value = ByteData {
                vec: Some(v.as_bytes().to_vec()),
            };
        } else if let Some(value_from) = &property.value_from {
            let kube_client = ActualKubeClient::new(kube_interface.clone());
            value = match self
                .get_discovery_property_value_from(&kube_client, value_from)
                .await
            {
                Ok(byte_data) => {
                    if byte_data.is_none() {
                        // optional value, not found
                        return Ok(None);
                    }
                    byte_data.unwrap()
                }
                Err(e) => return Err(e),
            };
        } else {
            // property without value
            value = ByteData { vec: None }
        }

        Ok(Some((property.name.clone(), value)))
    }

    async fn get_discovery_property_value_from(
        &self,
        kube_client: &dyn KubeClient,
        property: &DiscoveryPropertySource,
    ) -> anyhow::Result<Option<ByteData>> {
        match property {
            DiscoveryPropertySource::ConfigMapKeyRef(config_map_key_selector) => {
                get_discovery_property_value_from_config_map(kube_client, config_map_key_selector)
                    .await
            }
            DiscoveryPropertySource::SecretKeyRef(secret_key_selector) => {
                get_discovery_property_value_from_secret(kube_client, secret_key_selector).await
            }
        }
    }
}

async fn try_delete_instance(
    kube_interface: &dyn k8s::KubeInterface,
    instance_name: &str,
    instance_namespace: &str,
    node_name: String,
) -> Result<(), anyhow::Error> {
    for x in 0..MAX_INSTANCE_UPDATE_TRIES {
        // First check if instance still exists
        if let Ok(mut instance) = kube_interface
            .find_instance(instance_name, instance_namespace)
            .await
        {
            if instance.spec.nodes.contains(&node_name) {
                instance.spec.nodes.retain(|node| node != &node_name);
            }
            if instance.spec.nodes.is_empty() {
                match k8s::try_delete_instance(kube_interface, instance_name, instance_namespace)
                    .await
                {
                    Ok(()) => {
                        trace!("try_delete_instance - deleted Instance {}", instance_name);
                        break;
                    }
                    Err(e) => {
                        trace!("try_delete_instance - call to delete_instance returned with error {} on try # {} of {}", e, x, MAX_INSTANCE_UPDATE_TRIES);
                        if x == (MAX_INSTANCE_UPDATE_TRIES - 1) {
                            return Err(e);
                        }
                    }
                }
            } else {
                match kube_interface
                    .update_instance(
                        &instance.spec,
                        &instance.metadata.name.unwrap(),
                        instance_namespace,
                    )
                    .await
                {
                    Ok(()) => {
                        trace!(
                            "try_delete_instance - updated Instance {} to remove {}",
                            instance_name,
                            node_name
                        );
                        break;
                    }
                    Err(e) => {
                        trace!("try_delete_instance - call to update_instance returned with error {} on try # {} of {}", e, x, MAX_INSTANCE_UPDATE_TRIES);
                        if x == (MAX_INSTANCE_UPDATE_TRIES - 1) {
                            return Err(e);
                        }
                    }
                };
            }
        }
    }
    Ok(())
}

/// This provides a mockable way to query configMap and secret
#[cfg_attr(test, automock)]
#[tonic::async_trait]
pub trait KubeClient: Send + Sync {
    async fn get_secret(&self, name: &str, namespace: &str) -> anyhow::Result<Option<Secret>>;

    async fn get_config_map(
        &self,
        name: &str,
        namespace: &str,
    ) -> anyhow::Result<Option<ConfigMap>>;
}

struct ActualKubeClient {
    pub kube_interface: Arc<dyn k8s::KubeInterface>,
}

impl ActualKubeClient {
    pub fn new(kube_interface: Arc<dyn k8s::KubeInterface>) -> Self {
        ActualKubeClient { kube_interface }
    }
}

#[tonic::async_trait]
impl KubeClient for ActualKubeClient {
    async fn get_secret(&self, name: &str, namespace: &str) -> anyhow::Result<Option<Secret>> {
        let resource_client: Api<Secret> =
            Api::namespaced(self.kube_interface.get_kube_client(), namespace);
        let resource = resource_client.get_opt(name).await?;
        Ok(resource)
    }

    async fn get_config_map(
        &self,
        name: &str,
        namespace: &str,
    ) -> anyhow::Result<Option<ConfigMap>> {
        let resource_client: Api<ConfigMap> =
            Api::namespaced(self.kube_interface.get_kube_client(), namespace);
        let resource = resource_client.get_opt(name).await?;
        Ok(resource)
    }
}

async fn get_discovery_property_value_from_secret(
    kube_client: &dyn KubeClient,
    secret_key_selector: &DiscoveryPropertyKeySelector,
) -> anyhow::Result<Option<ByteData>> {
    let optional = secret_key_selector.optional.unwrap_or_default();
    let secret_name = &secret_key_selector.name;
    let secret_namespace = &secret_key_selector.namespace;
    let secret_key = &secret_key_selector.key;

    let secret = kube_client
        .get_secret(secret_name, secret_namespace)
        .await?;
    if secret.is_none() {
        if optional {
            return Ok(None);
        } else {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "discoveryProperties' referenced Secret not found",
            )
            .into());
        }
    }
    let secret = secret.unwrap();
    // All key-value pairs in the stringData field are internally merged into the data field
    // we don't need to check string_data.
    if let Some(data) = secret.data {
        if let Some(v) = data.get(secret_key) {
            return Ok(Some(ByteData {
                vec: Some(v.0.clone()),
            }));
        }
    }

    // secret key/value not found
    if optional {
        Ok(None)
    } else {
        Err(Error::new(
            ErrorKind::InvalidInput,
            "discoveryProperties' referenced Secret data not found",
        )
        .into())
    }
}

async fn get_discovery_property_value_from_config_map(
    kube_client: &dyn KubeClient,
    config_map_key_selector: &DiscoveryPropertyKeySelector,
) -> anyhow::Result<Option<ByteData>> {
    let optional = config_map_key_selector.optional.unwrap_or_default();
    let config_map_name = &config_map_key_selector.name;
    let config_map_namespace = &config_map_key_selector.namespace;
    let config_map_key = &config_map_key_selector.key;

    let config_map = kube_client
        .get_config_map(config_map_name, config_map_namespace)
        .await?;
    if config_map.is_none() {
        if optional {
            return Ok(None);
        } else {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "discoveryProperties' referenced ConfigMap not found",
            )
            .into());
        }
    }
    let config_map = config_map.unwrap();
    if let Some(data) = config_map.data {
        if let Some(v) = data.get(config_map_key) {
            return Ok(Some(ByteData {
                vec: Some(v.as_bytes().to_vec()),
            }));
        }
    }
    if let Some(binary_data) = config_map.binary_data {
        if let Some(v) = binary_data.get(config_map_key) {
            return Ok(Some(ByteData {
                vec: Some(v.0.clone()),
            }));
        }
    }

    // config_map key/value not found
    if optional {
        Ok(None)
    } else {
        Err(Error::new(
            ErrorKind::InvalidInput,
            "discoveryProperties' referenced ConfigMap data not found",
        )
        .into())
    }
}

pub mod start_discovery {
    use super::super::metrics::{DISCOVERY_RESPONSE_RESULT_METRIC, DISCOVERY_RESPONSE_TIME_METRIC};
    use super::super::registration::{DiscoveryDetails, DiscoveryHandlerEndpoint};
    // Use this `mockall` macro to automate importing a mock type in test mode, or a real type otherwise.
    use super::super::device_plugin_builder::{DevicePluginBuilder, DevicePluginBuilderInterface};
    use super::device_plugin_service::get_device_configuration_name;
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
        kube_interface: Arc<dyn k8s::KubeInterface>,
        node_name: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        internal_start_discovery(
            discovery_operator,
            new_discovery_handler_sender,
            stop_all_discovery_sender,
            finished_all_discovery_sender,
            kube_interface,
            Box::new(DevicePluginBuilder {}),
            node_name,
        )
        .await
    }

    pub async fn internal_start_discovery(
        discovery_operator: DiscoveryOperator,
        new_discovery_handler_sender: broadcast::Sender<String>,
        stop_all_discovery_sender: broadcast::Sender<()>,
        finished_all_discovery_sender: &mut mpsc::Sender<()>,
        kube_interface: Arc<dyn k8s::KubeInterface>,
        device_plugin_builder: Box<dyn DevicePluginBuilderInterface>,
        node_name: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        let config = discovery_operator.get_config();
        info!(
            "internal_start_discovery - entered for {} discovery handler",
            config.spec.discovery_handler.name
        );
        let config_name = config.metadata.name.clone().unwrap();
        let mut tasks = Vec::new();
        let device_plugin_context = discovery_operator.get_device_plugin_context();
        let discovery_operator = Arc::new(discovery_operator);

        // Create a device plugin for the Configuration
        let config_dp_name = get_device_configuration_name(&config_name);
        trace!(
            "internal_start_discovery - create configuration device plugin {}",
            config_dp_name
        );
        match device_plugin_builder
            .build_configuration_device_plugin(
                config_dp_name,
                &config,
                device_plugin_context.clone(),
                node_name.clone(),
            )
            .await
        {
            Ok(s) => {
                device_plugin_context
                    .write()
                    .await
                    .usage_update_message_sender = Some(s);
            }
            Err(e) => {
                error!(
                    "internal_start_discovery - error {} building configuration device plugin",
                    e
                );
            }
        };

        // Call discover on already registered Discovery Handlers requested by this Configuration's
        let known_dh_discovery_operator = discovery_operator.clone();
        let known_dh_kube_interface = kube_interface.clone();
        let known_node_name = node_name.clone();
        tasks.push(tokio::spawn(async move {
            do_discover(
                known_dh_discovery_operator,
                known_dh_kube_interface,
                known_node_name,
            )
            .await
            .unwrap();
        }));

        // Listen for new discovery handlers to call discover on
        let mut stop_all_discovery_receiver = stop_all_discovery_sender.subscribe();
        let mut new_discovery_handler_receiver = new_discovery_handler_sender.subscribe();
        let new_dh_discovery_operator = discovery_operator.clone();
        let new_node_name = node_name.clone();
        tasks.push(tokio::spawn(async move {
            listen_for_new_discovery_handlers(
                new_dh_discovery_operator,
                &mut new_discovery_handler_receiver,
                &mut stop_all_discovery_receiver,
                new_node_name,
            )
            .await
            .unwrap();
        }));

        // Non-local devices are only allowed to be offline for `SHARED_INSTANCE_OFFLINE_GRACE_PERIOD_SECS` minutes before being removed.
        // This task periodically checks if devices have been offline for too long.
        let mut stop_all_discovery_receiver = stop_all_discovery_sender.subscribe();
        let offline_dh_discovery_operator = discovery_operator.clone();
        let offline_dh_kube_interface = kube_interface.clone();
        let offline_node_name = node_name.clone();
        tasks.push(tokio::spawn(async move {
            loop {
                offline_dh_discovery_operator
                    .delete_offline_instances(offline_dh_kube_interface.clone(), offline_node_name.clone())
                    .await
                    .unwrap();
                if tokio::time::timeout(
                    Duration::from_secs(30),
                    stop_all_discovery_receiver.recv(),
                )
                .await.is_ok()
                {
                    trace!("internal_start_discovery - received message to stop checking connectivity status for configuration {}", config_name);
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
        node_name: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        let mut discovery_tasks = Vec::new();
        loop {
            tokio::select! {
                _ = stop_all_discovery_receiver.recv() => {
                    trace!("listen_for_new_discovery_handlers - received message to stop discovery for configuration {:?}", discovery_operator.get_config().metadata.name);
                    discovery_operator.stop_all_discovery().await;
                    break;
                },
                result = new_discovery_handler_receiver.recv() => {
                    // Check if it is one of this Configuration's discovery handlers
                    if let Ok(discovery_handler_name) = result {
                        if discovery_handler_name == discovery_operator.get_config().spec.discovery_handler.name {
                            trace!("listen_for_new_discovery_handlers - received new registered discovery handler for configuration {:?}", discovery_operator.get_config().metadata.name);
                            let new_discovery_operator = discovery_operator.clone();
                            let node_name = node_name.clone();
                            discovery_tasks.push(tokio::spawn(async move {
                                do_discover(new_discovery_operator, Arc::new(k8s::KubeImpl::new().await.unwrap()), node_name.clone()).await.unwrap();
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
    /// If a connection cannot be established, continues to try, sleeping between iteration.
    pub async fn do_discover(
        discovery_operator: Arc<DiscoveryOperator>,
        kube_interface: Arc<dyn k8s::KubeInterface>,
        node_name: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        let mut discovery_tasks = Vec::new();
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
                let discovery_operator = discovery_operator.clone();
                let kube_interface = kube_interface.clone();
                let node_name = node_name.clone();
                discovery_tasks.push(tokio::spawn(async move {
                    do_discover_on_discovery_handler(
                        discovery_operator.clone(),
                        kube_interface.clone(),
                        &endpoint,
                        &dh_details,
                        node_name.clone(),
                    )
                    .await
                    .unwrap();
                }));
            }
        }
        futures::future::try_join_all(discovery_tasks).await?;
        Ok(())
    }

    /// Try to connect to discovery handler until connection has been established or grace period has passed
    async fn do_discover_on_discovery_handler<'a>(
        discovery_operator: Arc<DiscoveryOperator>,
        kube_interface: Arc<dyn k8s::KubeInterface>,
        endpoint: &'a DiscoveryHandlerEndpoint,
        dh_details: &'a DiscoveryDetails,
        node_name: String,
    ) -> anyhow::Result<()> {
        // get discovery handler name for metric use
        let dh_name = discovery_operator.get_config().spec.discovery_handler.name;
        let (_config_namespace, config_name) = discovery_operator.get_config_id();
        let mut first_call = true;
        loop {
            let stream_type = discovery_operator
                .get_stream(kube_interface.clone(), endpoint)
                .await;
            let request_result = stream_type.as_ref().map(|_| "Success").unwrap_or("Fail");
            DISCOVERY_RESPONSE_RESULT_METRIC
                .with_label_values(&[&dh_name, request_result])
                .inc();
            if first_call {
                first_call = false;
                let start_time = discovery_operator.get_config_timestamp();
                DISCOVERY_RESPONSE_TIME_METRIC
                    .with_label_values(&[&config_name])
                    .observe(start_time.elapsed().as_secs_f64());
            }
            if let Some(stream_type) = stream_type {
                match stream_type {
                    StreamType::External(mut stream) => {
                        match discovery_operator
                            .internal_do_discover(
                                kube_interface.clone(),
                                dh_details,
                                &mut stream,
                                node_name.clone(),
                            )
                            .await
                        {
                            Ok(_) => {
                                break;
                            }
                            Err(e) => {
                                if let Some(status) = e.downcast_ref::<tonic::Status>() {
                                    if status.message().contains("broken pipe") {
                                        // Mark all associated instances as offline
                                        error!("do_discover_on_discovery_handler - connection with Discovery Handler dropped with status {:?}. Marking all instances offline.", status);
                                        discovery_operator
                                            .update_instance_connectivity_status(
                                                kube_interface.clone(),
                                                std::collections::HashMap::new(),
                                                dh_details.shared,
                                                node_name.clone(),
                                            )
                                            .await?;
                                    } else {
                                        trace!("do_discover_on_discovery_handler - Discovery Handlers returned error status {}. Marking all instances offline.", status);
                                        // TODO: Possibly mark config as invalid
                                        discovery_operator
                                            .update_instance_connectivity_status(
                                                kube_interface.clone(),
                                                std::collections::HashMap::new(),
                                                dh_details.shared,
                                                node_name.clone(),
                                            )
                                            .await?;
                                    }
                                } else {
                                    return Err(e);
                                }
                            }
                        }
                    }
                    #[cfg(any(test, feature = "agent-full"))]
                    StreamType::Embedded(mut stream) => {
                        discovery_operator
                            .internal_do_discover(
                                kube_interface.clone(),
                                dh_details,
                                &mut stream,
                                node_name.clone(),
                            )
                            .await?;
                        // Embedded discovery should only return okay if signaled to stop. Otherwise, bubble up error.
                        break;
                    }
                }
            }

            // If a connection cannot be established with the Discovery Handler, it will sleep and try again.
            // This continues until connection established or the Discovery Handler is told to stop discovery.
            let mut stop_discovery_receiver =
                dh_details.close_discovery_handler_connection.subscribe();
            let mut sleep_duration = Duration::from_secs(60);
            if cfg!(test) {
                sleep_duration = Duration::from_millis(100);
            }

            if let Ok(result) =
                tokio::time::timeout(sleep_duration, stop_discovery_receiver.recv()).await
            {
                // Stop is triggered if the current config_id (to only stop this task) or None (to stop all tasks of this handler) is sent.
                if let Ok(Some(config_id)) = result {
                    if config_id != discovery_operator.get_config_id() {
                        trace!("do_discover_on_discovery_handler - received message to stop discovery for another configuration, ignoring it.");
                        continue;
                    }
                }
                let (config_namespace, config_name) = discovery_operator.get_config_id();
                trace!(
                    "do_discover_on_discovery_handler({}::{}) - received message to stop discovery for {} Discovery Handler at endpoint {:?}", 
                    config_namespace, config_name,
                    dh_details.name, dh_details.endpoint,
                );
                break;
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
pub fn generate_instance_digest(id_to_digest: &str, shared: bool, node_name: &str) -> String {
    let mut id_to_digest = id_to_digest.to_string();
    // For local devices, include node hostname in id_to_digest so instances have unique names
    if !shared {
        id_to_digest = format!("{}{}", &id_to_digest, node_name,);
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
        registration::{inner_register_embedded_discovery_handlers, DiscoveryDetails},
    };
    use super::device_plugin_service::DevicePluginContext;
    use super::*;
    use akri_discovery_utils::discovery::mock_discovery_handler;
    use akri_shared::{
        akri::configuration::Configuration, k8s::MockKubeInterface, os::env_var::MockEnvVarQuery,
    };
    use k8s_openapi::ByteString;
    use mock_instant::{Instant, MockClock};
    use mockall::Sequence;
    use std::collections::BTreeMap;
    use std::time::Duration;
    use tokio::sync::{broadcast, mpsc};

    pub async fn build_device_plugin_context(
        config: &Configuration,
        visible_discovery_results: &mut Vec<Device>,
        list_and_watch_message_receivers: &mut Vec<
            broadcast::Receiver<device_plugin_service::ListAndWatchMessageKind>,
        >,
        connectivity_status: InstanceConnectivityStatus,
    ) -> Arc<RwLock<DevicePluginContext>> {
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
            config.metadata.name.as_ref().unwrap(),
        )
    }

    fn generate_instance_map(
        discovery_results: Vec<Device>,
        list_and_watch_message_receivers: &mut Vec<
            broadcast::Receiver<device_plugin_service::ListAndWatchMessageKind>,
        >,
        connectivity_status: InstanceConnectivityStatus,
        config_name: &str,
    ) -> Arc<RwLock<DevicePluginContext>> {
        Arc::new(RwLock::new(DevicePluginContext {
            usage_update_message_sender: None,
            instances: discovery_results
                .iter()
                .map(|device| {
                    let (list_and_watch_message_sender, list_and_watch_message_receiver) =
                        broadcast::channel(2);
                    list_and_watch_message_receivers.push(list_and_watch_message_receiver);
                    let instance_name = get_device_instance_name(&device.id, config_name);
                    (
                        instance_name,
                        InstanceInfo {
                            list_and_watch_message_sender,
                            connectivity_status: connectivity_status.clone(),
                            instance_id: device.id.clone(),
                            device: device.clone(),
                        },
                    )
                })
                .collect(),
        }))
    }

    fn create_mock_discovery_operator(
        discovery_handler_map: RegisteredDiscoveryHandlerMap,
        config: Configuration,
        device_plugin_context: Arc<RwLock<DevicePluginContext>>,
    ) -> MockDiscoveryOperator {
        let ctx = MockDiscoveryOperator::new_context();
        let discovery_handler_map_clone = discovery_handler_map.clone();
        let config_clone = config.clone();
        let config_id = (
            config.metadata.namespace.clone().unwrap(),
            config.metadata.namespace.clone().unwrap(),
        );
        let device_plugin_context_clone = device_plugin_context.clone();
        ctx.expect().return_once(move |_, _, _| {
            // let mut discovery_handler_status_seq = Sequence::new();
            let mut mock = MockDiscoveryOperator::default();
            mock.expect_get_discovery_handler_map()
                .returning(move || discovery_handler_map_clone.clone());
            mock.expect_get_config()
                .returning(move || config_clone.clone());
            mock.expect_get_device_plugin_context()
                .returning(move || device_plugin_context_clone.clone());
            mock.expect_get_config_id()
                .returning(move || config_id.clone());
            mock
        });
        MockDiscoveryOperator::new(discovery_handler_map, config, device_plugin_context)
    }

    // Creates a discovery handler with specified properties and adds it to the RegisteredDiscoveryHandlerMap.
    pub fn add_discovery_handler_to_map(
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
                if !dh_details_map.contains_key(endpoint) {
                    dh_details_map.insert(endpoint.clone(), discovery_handler_details);
                }
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
            close_discovery_handler_connection,
        }
    }

    fn setup_test_do_discover(
        config_name: &str,
        discovery_handler_map: RegisteredDiscoveryHandlerMap,
    ) -> MockDiscoveryOperator {
        add_discovery_handler_to_map(
            "debugEcho",
            &DiscoveryHandlerEndpoint::Uds("socket.sock".to_string()),
            false,
            discovery_handler_map.clone(),
        );

        // Build discovery operator
        let path_to_config = "../test/yaml/config-a.yaml";
        let config_yaml = std::fs::read_to_string(path_to_config).expect("Unable to read file");
        let mut config: Configuration = serde_yaml::from_str(&config_yaml).unwrap();
        config.metadata.name = Some(config_name.to_string());
        config.metadata.namespace = Some(config_name.to_string());
        create_mock_discovery_operator(
            discovery_handler_map,
            config,
            Arc::new(RwLock::new(DevicePluginContext::default())),
        )
    }

    #[test]
    fn test_generate_instance_digest() {
        let id = "video1";
        let first_unshared_video_digest = generate_instance_digest(id, false, "node-a");
        let first_shared_video_digest = generate_instance_digest(id, true, "node-a");

        let second_unshared_video_digest = generate_instance_digest(id, false, "node-b");
        let second_shared_video_digest = generate_instance_digest(id, true, "node-b");
        // unshared instances visible to different nodes should NOT have the same digest
        assert_ne!(first_unshared_video_digest, second_unshared_video_digest);
        // shared instances visible to different nodes should have the same digest
        assert_eq!(first_shared_video_digest, second_shared_video_digest);
        // shared and unshared instance for same node should NOT have the same digest
        assert_ne!(first_unshared_video_digest, first_shared_video_digest);
    }

    #[tokio::test]
    async fn test_internal_do_discover_stop_one() {
        let mock_kube_interface1: Arc<dyn k8s::KubeInterface> = Arc::new(MockKubeInterface::new());
        let mock_kube_interface2 = mock_kube_interface1.clone();
        let dh_name = "debugEcho";
        let discovery_handler_map = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let endpoint = DiscoveryHandlerEndpoint::Uds("socket.sock".to_string());
        add_discovery_handler_to_map(dh_name, &endpoint, false, discovery_handler_map.clone());
        let dh_details1 = discovery_handler_map
            .lock()
            .unwrap()
            .get(dh_name)
            .unwrap()
            .get(&endpoint)
            .unwrap()
            .clone();
        let dh_details2 = dh_details1.clone();

        let (_tx1, mut rx1) = mpsc::channel(2);
        let (_tx2, mut rx2) = mpsc::channel(2);

        let config1: Configuration = serde_yaml::from_str(
            std::fs::read_to_string("../test/yaml/config-a.yaml")
                .expect("Unable to read file")
                .as_str(),
        )
        .unwrap();
        let discovery_operator1 = Arc::new(DiscoveryOperator::new(
            discovery_handler_map.clone(),
            config1,
            Arc::new(RwLock::new(DevicePluginContext::default())),
        ));
        let local_do1 = discovery_operator1.clone();
        let discover1 = tokio::spawn(async move {
            discovery_operator1
                .internal_do_discover(
                    mock_kube_interface1,
                    &dh_details1,
                    &mut rx1,
                    "node-a".to_string(),
                )
                .await
                .unwrap()
        });

        let config2: Configuration = serde_yaml::from_str(
            std::fs::read_to_string("../test/yaml/config-b.yaml")
                .expect("Unable to read file")
                .as_str(),
        )
        .unwrap();
        let discovery_operator2 = Arc::new(DiscoveryOperator::new(
            discovery_handler_map,
            config2,
            Arc::new(RwLock::new(DevicePluginContext::default())),
        ));
        let discover2 = tokio::spawn(async move {
            discovery_operator2
                .internal_do_discover(
                    mock_kube_interface2,
                    &dh_details2,
                    &mut rx2,
                    "node-a".to_string(),
                )
                .await
                .unwrap()
        });
        tokio::time::sleep(Duration::from_millis(100)).await; // Make sure they had time to launch
        local_do1.stop_all_discovery().await;
        assert!(tokio::time::timeout(Duration::from_millis(100), discover1)
            .await
            .is_ok());
        assert!(tokio::time::timeout(Duration::from_millis(100), discover2)
            .await
            .is_err());
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
        let config: Configuration = serde_yaml::from_str(&config_yaml).unwrap();
        let discovery_operator = Arc::new(DiscoveryOperator::new(
            discovery_handler_map,
            config,
            Arc::new(RwLock::new(DevicePluginContext::default())),
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
        let discovery_handler_map = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let mut start_discovery_components =
            start_discovery_setup("config-a", true, discovery_handler_map).await;
        start_discovery_components
            .running_receiver
            .recv()
            .await
            .unwrap();
        start_discovery_components
            .stop_all_discovery_sender
            .send(())
            .unwrap();
        start_discovery_components
            .finished_discovery_receiver
            .recv()
            .await
            .unwrap();
        // Make sure that all threads have finished
        start_discovery_components
            .start_discovery_handle
            .await
            .unwrap();
    }

    // Test that start discovery can be called twice for two (differently named)
    // Configurations that use the same DH.
    #[tokio::test]
    async fn test_start_discovery_same_discovery_handler() {
        let _ = env_logger::builder().is_test(true).try_init();
        let discovery_handler_map = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let mut start_discovery_components_a =
            start_discovery_setup("config-a", false, discovery_handler_map.clone()).await;
        let mut start_discovery_components_b =
            start_discovery_setup("config-b", false, discovery_handler_map.clone()).await;

        start_discovery_components_a
            .running_receiver
            .recv()
            .await
            .unwrap();
        start_discovery_components_b
            .running_receiver
            .recv()
            .await
            .unwrap();
    }

    struct StartDiscoveryComponents {
        finished_discovery_receiver: tokio::sync::mpsc::Receiver<()>,
        stop_all_discovery_sender: tokio::sync::broadcast::Sender<()>,
        running_receiver: tokio::sync::broadcast::Receiver<()>,
        start_discovery_handle: tokio::task::JoinHandle<()>,
    }

    async fn start_discovery_setup(
        config_name: &str,
        terminate: bool,
        discovery_handler_map: RegisteredDiscoveryHandlerMap,
    ) -> StartDiscoveryComponents {
        let mut mock_discovery_operator =
            setup_test_do_discover(config_name, discovery_handler_map.clone());
        let (running_sender, running_receiver) = tokio::sync::broadcast::channel::<()>(1);
        mock_discovery_operator
            .expect_get_stream()
            .returning(move |_, _| {
                running_sender.clone().send(()).unwrap();
                None
            });

        mock_discovery_operator
            .expect_delete_offline_instances()
            .times(1)
            .returning(move |_, _| Ok(()));
        if terminate {
            let stop_dh_discovery_sender = discovery_handler_map
                .lock()
                .unwrap()
                .get_mut("debugEcho")
                .unwrap()
                .clone()
                .get(&DiscoveryHandlerEndpoint::Uds("socket.sock".to_string()))
                .unwrap()
                .clone()
                .close_discovery_handler_connection;
            let local_config_id = mock_discovery_operator.get_config_id();
            mock_discovery_operator
                .expect_stop_all_discovery()
                .times(1)
                .returning(move || {
                    stop_dh_discovery_sender
                        .clone()
                        .send(Some(local_config_id.clone()))
                        .unwrap();
                });
        }
        // Config timestamp should be called
        mock_discovery_operator
            .expect_get_config_timestamp()
            .times(1)
            .returning(Instant::now);
        let (mut finished_discovery_sender, finished_discovery_receiver) =
            tokio::sync::mpsc::channel(2);
        let (new_dh_sender, _) = broadcast::channel(2);
        let (stop_all_discovery_sender, _) = broadcast::channel(2);
        let thread_stop_all_discovery_sender = stop_all_discovery_sender.clone();
        let mock_kube_interface: Arc<dyn k8s::KubeInterface> = Arc::new(MockKubeInterface::new());
        let mut mock_device_plugin_builder = Box::new(MockDevicePluginBuilderInterface::new());
        mock_device_plugin_builder
            .expect_build_configuration_device_plugin()
            .times(1)
            .returning(move |_, _, _, _| {
                let (sender, _) = broadcast::channel(2);
                Ok(sender)
            });

        let start_discovery_handle = tokio::spawn(async move {
            start_discovery::internal_start_discovery(
                mock_discovery_operator,
                new_dh_sender.to_owned(),
                thread_stop_all_discovery_sender,
                &mut finished_discovery_sender,
                mock_kube_interface,
                mock_device_plugin_builder,
                "node-a".to_string(),
            )
            .await
            .unwrap();
        });
        StartDiscoveryComponents {
            finished_discovery_receiver,
            stop_all_discovery_sender,
            running_receiver,
            start_discovery_handle,
        }
    }

    // Test that DH is connected to on second try getting stream.
    #[tokio::test]
    async fn test_do_discover_completed_internal_connection() {
        let _ = env_logger::builder().is_test(true).try_init();
        let discovery_handler_map = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let mut mock_discovery_operator = setup_test_do_discover("config-a", discovery_handler_map);
        let mut get_stream_seq = Sequence::new();
        // First time cannot get stream
        mock_discovery_operator
            .expect_get_stream()
            .times(1)
            .returning(|_, _| None)
            .in_sequence(&mut get_stream_seq);
        // Second time successfully get stream
        let (_, rx) = mpsc::channel(2);
        let stream_type = Some(StreamType::Embedded(rx));
        mock_discovery_operator
            .expect_get_stream()
            .times(1)
            .return_once(move |_, _| stream_type)
            .in_sequence(&mut get_stream_seq);
        // Discovery should be initiated
        mock_discovery_operator
            .expect_internal_do_discover()
            .times(1)
            .returning(|_, _, _, _| Ok(()));
        // Config timestamp should be called
        mock_discovery_operator
            .expect_get_config_timestamp()
            .times(1)
            .returning(Instant::now);
        let mock_kube_interface: Arc<dyn k8s::KubeInterface> = Arc::new(MockKubeInterface::new());
        start_discovery::do_discover(
            Arc::new(mock_discovery_operator),
            mock_kube_interface,
            "node-a".to_string(),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_handle_discovery_results() {
        let _ = env_logger::builder().is_test(true).try_init();
        let mock_kube_interface: Arc<dyn k8s::KubeInterface> = Arc::new(MockKubeInterface::new());
        let discovery_handler_map: RegisteredDiscoveryHandlerMap =
            Arc::new(std::sync::Mutex::new(HashMap::new()));
        let path_to_config = "../test/yaml/config-a.yaml";
        let config_yaml = std::fs::read_to_string(path_to_config).expect("Unable to read file");
        let config: Configuration = serde_yaml::from_str(&config_yaml).unwrap();
        let config_name = config.metadata.name.clone().unwrap();
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
            Arc::new(RwLock::new(DevicePluginContext::default())),
        ));
        let mut mock_device_plugin_builder = MockDevicePluginBuilderInterface::new();
        mock_device_plugin_builder
            .expect_build_device_plugin()
            .times(2)
            .returning(move |_, _, _, _, _, _| Ok(()));
        discovery_operator
            .handle_discovery_results(
                mock_kube_interface,
                discovery_results,
                true,
                Box::new(mock_device_plugin_builder),
                "node-a".to_string(),
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
        device_plugin_context: Arc<RwLock<DevicePluginContext>>,
        check_empty: bool,
    ) {
        let mut keep_looping = false;
        let mut map_is_empty = false;
        let tries: i8 = 5;
        for _x in 0..tries {
            println!("try number {}", _x);
            keep_looping = false;
            tokio::time::sleep(Duration::from_millis(100)).await;
            let unwrapped_device_plugin_context = device_plugin_context.read().await.clone();
            if check_empty && unwrapped_device_plugin_context.instances.is_empty() {
                map_is_empty = true;
                break;
            }
            for (_, instance_info) in unwrapped_device_plugin_context.instances {
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

    fn get_test_instance(nodes: Vec<&str>) -> akri_shared::akri::instance::Instance {
        let nodes = nodes.into_iter().map(|e| e.to_string()).collect();
        let mut instance: akri_shared::akri::instance::Instance = serde_json::from_str(
            r#"
        {
            "apiVersion": "akri.sh/v0",
            "kind": "Instance",
            "metadata": {
                "name": "foo",
                "namespace": "bar",
                "uid": "abcdegfh-ijkl-mnop-qrst-uvwxyz012345"
            },
            "spec": {
                "configurationName": "",
                "nodes": [],
                "shared": true,
                "deviceUsage": {}
            }
        }
        "#,
        )
        .unwrap();
        instance.spec.nodes = nodes;
        instance
    }

    #[tokio::test]
    async fn test_delete_offline_instances() {
        let _ = env_logger::builder().is_test(true).try_init();
        let path_to_config = "../test/yaml/config-a.yaml";
        let config_yaml = std::fs::read_to_string(path_to_config).expect("Unable to read file");
        let config: Configuration = serde_yaml::from_str(&config_yaml).unwrap();
        let mut list_and_watch_message_receivers = Vec::new();
        let discovery_handler_map: RegisteredDiscoveryHandlerMap =
            Arc::new(std::sync::Mutex::new(HashMap::new()));
        let mut visible_discovery_results = Vec::new();

        // Assert no action (to delete instances by mock kube interface) is taken for all online instances
        let device_plugin_context = build_device_plugin_context(
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
            device_plugin_context,
        ));
        discovery_operator
            .delete_offline_instances(Arc::new(mock), "node-a".to_string())
            .await
            .unwrap();

        // Assert no action (to delete instances by mock kube interface) is taken for instances offline for less than grace period
        let mock_now = Instant::now();
        MockClock::advance(Duration::from_secs(30));
        let device_plugin_context = build_device_plugin_context(
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
            device_plugin_context,
        ));
        discovery_operator
            .delete_offline_instances(Arc::new(mock), "node-a".to_string())
            .await
            .unwrap();

        // Assert that all instances that have been offline for more than 5 minutes are deleted
        let mock_now = Instant::now();
        MockClock::advance(Duration::from_secs(301));
        let device_plugin_context = build_device_plugin_context(
            &config,
            &mut visible_discovery_results,
            &mut list_and_watch_message_receivers,
            InstanceConnectivityStatus::Offline(mock_now),
        )
        .await;
        let mut mock = MockKubeInterface::new();
        let instance = get_test_instance(vec![]);
        mock.expect_find_instance()
            .times(2)
            .returning(move |_, _| Ok(instance.clone()));
        mock.expect_delete_instance()
            .times(2)
            .returning(move |_, _| Ok(()));
        let discovery_operator = Arc::new(DiscoveryOperator::new(
            discovery_handler_map.clone(),
            config.clone(),
            device_plugin_context.clone(),
        ));
        discovery_operator
            .delete_offline_instances(Arc::new(mock), "node-a".to_string())
            .await
            .unwrap();

        // Make sure all instances are deleted from map. Note, first 3 arguments are ignored.
        check_status_or_empty_loop(
            InstanceConnectivityStatus::Online,
            true,
            device_plugin_context,
            true,
        )
        .await;
    }

    // 1: InstanceConnectivityStatus of all instances that go offline is changed from Online to Offline
    // 2: InstanceConnectivityStatus of shared instances that come back online in under 5 minutes is changed from Offline to Online
    // 3: InstanceConnectivityStatus of unshared instances that come back online before next periodic discovery is changed from Offline to Online
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_update_instance_connectivity_status_factory() {
        let _ = env_logger::builder().is_test(true).try_init();
        let path_to_config = "../test/yaml/config-a.yaml";
        let config_yaml = std::fs::read_to_string(path_to_config).expect("Unable to read file");
        let config: Configuration = serde_yaml::from_str(&config_yaml).unwrap();
        let config_name = config.metadata.name.clone().unwrap();
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
        let device_plugin_context = build_device_plugin_context(
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
            device_plugin_context.clone(),
            discovery_handler_map.clone(),
            MockKubeInterface::new(),
        )
        .await;

        // Check that no instances are still online
        check_status_or_empty_loop(
            InstanceConnectivityStatus::Online,
            false,
            device_plugin_context,
            false,
        )
        .await;

        //
        // 2: Assert that InstanceConnectivityStatus of shared instances that come back online in <5 mins is changed to Online
        //
        let mock_now = Instant::now();
        MockClock::advance(Duration::from_secs(30));
        let device_plugin_context = build_device_plugin_context(
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
            device_plugin_context.clone(),
            discovery_handler_map.clone(),
            MockKubeInterface::new(),
        )
        .await;

        // Check that all instances marked online
        check_status_or_empty_loop(
            InstanceConnectivityStatus::Online,
            true,
            device_plugin_context,
            false,
        )
        .await;

        //
        // 3: Assert that shared instances that are offline for more than 5 minutes are removed from the instance map
        //
        let mock_now = Instant::now();
        MockClock::advance(Duration::from_secs(301));
        let device_plugin_context = build_device_plugin_context(
            &config,
            &mut visible_discovery_results,
            &mut list_and_watch_message_receivers,
            InstanceConnectivityStatus::Offline(mock_now),
        )
        .await;
        let mut mock = MockKubeInterface::new();
        let instance = get_test_instance(vec![]);
        mock.expect_find_instance()
            .times(2)
            .returning(move |_, _| Ok(instance.clone()));
        mock.expect_delete_instance()
            .times(2)
            .returning(move |_, _| Ok(()));
        let shared = true;
        run_update_instance_connectivity_status(
            config.clone(),
            HashMap::new(),
            shared,
            device_plugin_context.clone(),
            discovery_handler_map.clone(),
            mock,
        )
        .await;
        // Make sure all instances are deleted from map. Note, first 3 arguments are ignored.
        check_status_or_empty_loop(
            InstanceConnectivityStatus::Online,
            true,
            device_plugin_context,
            true,
        )
        .await;

        //
        // 4: Assert that local devices that go offline are removed from the instance map
        //
        let mut mock = MockKubeInterface::new();
        let instance = get_test_instance(vec![]);
        mock.expect_find_instance()
            .times(2)
            .returning(move |_, _| Ok(instance.clone()));
        mock.expect_delete_instance()
            .times(2)
            .returning(move |_, _| Ok(()));

        let device_plugin_context = build_device_plugin_context(
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
            device_plugin_context.clone(),
            discovery_handler_map.clone(),
            mock,
        )
        .await;
        // Make sure all instances are deleted from map. Note, first 3 arguments are ignored.
        check_status_or_empty_loop(
            InstanceConnectivityStatus::Online,
            true,
            device_plugin_context,
            true,
        )
        .await;
    }

    async fn run_update_instance_connectivity_status(
        config: Configuration,
        currently_visible_instances: HashMap<String, Device>,
        shared: bool,
        device_plugin_context: Arc<RwLock<DevicePluginContext>>,
        discovery_handler_map: RegisteredDiscoveryHandlerMap,
        mock: MockKubeInterface,
    ) {
        let discovery_operator = Arc::new(DiscoveryOperator::new(
            discovery_handler_map,
            config,
            device_plugin_context.clone(),
        ));
        discovery_operator
            .update_instance_connectivity_status(
                Arc::new(mock),
                currently_visible_instances,
                shared,
                "node-a".to_string(),
            )
            .await
            .unwrap();
    }

    fn create_discovery_operator(path_to_config: &str) -> DiscoveryOperator {
        let config_yaml = std::fs::read_to_string(path_to_config).expect("Unable to read file");
        let config: Configuration = serde_yaml::from_str(&config_yaml).unwrap();
        let discovery_handler_map = Arc::new(std::sync::Mutex::new(HashMap::new()));
        DiscoveryOperator::new(
            discovery_handler_map,
            config,
            Arc::new(RwLock::new(DevicePluginContext::default())),
        )
    }

    fn setup_non_mocked_dh(
        dh_name: &str,
        endpoint: &DiscoveryHandlerEndpoint,
    ) -> DiscoveryOperator {
        let discovery_operator = create_discovery_operator("../test/yaml/config-a.yaml");
        add_discovery_handler_to_map(
            dh_name,
            endpoint,
            false,
            discovery_operator.discovery_handler_map.clone(),
        );
        discovery_operator
    }

    #[tokio::test]
    async fn test_get_stream_embedded() {
        let _ = env_logger::builder().is_test(true).try_init();
        std::env::set_var(super::super::constants::ENABLE_DEBUG_ECHO_LABEL, "yes");
        let path_to_config = "../test/yaml/config-a.yaml";
        let config_yaml = std::fs::read_to_string(path_to_config).expect("Unable to read file");
        let config: Configuration = serde_yaml::from_str(&config_yaml).unwrap();
        let discovery_handler_map = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let endpoint = DiscoveryHandlerEndpoint::Embedded;
        let dh_name = akri_debug_echo::DISCOVERY_HANDLER_NAME.to_string();
        add_discovery_handler_to_map(&dh_name, &endpoint, false, discovery_handler_map.clone());
        let discovery_operator = DiscoveryOperator::new(
            discovery_handler_map,
            config,
            Arc::new(RwLock::new(DevicePluginContext::default())),
        );
        let mock_kube_interface: Arc<dyn k8s::KubeInterface> = Arc::new(MockKubeInterface::new());

        // test embedded debugEcho socket
        if let Some(StreamType::Embedded(_)) = discovery_operator
            .get_stream(mock_kube_interface, &DiscoveryHandlerEndpoint::Embedded)
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
            Vec::new(),
        )
        .await;
        // Make sure registration server has started
        akri_shared::uds::unix_stream::try_connect(endpoint)
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
        let mock_kube_interface: Arc<dyn k8s::KubeInterface> = Arc::new(MockKubeInterface::new());

        // Should not be able to get stream if DH is not running
        assert!(discovery_operator
            .get_stream(mock_kube_interface, &dh_endpoint)
            .await
            .is_none());
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
        let mock_kube_interface: Arc<dyn k8s::KubeInterface> = Arc::new(MockKubeInterface::new());

        // Assert that get_stream returns none if the DH returns error
        assert!(discovery_operator
            .get_stream(mock_kube_interface, &dh_endpoint)
            .await
            .is_none());
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
        let mock_kube_interface: Arc<dyn k8s::KubeInterface> = Arc::new(MockKubeInterface::new());

        if let Some(StreamType::External(mut receiver)) = discovery_operator
            .get_stream(mock_kube_interface, &dh_endpoint)
            .await
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

    #[tokio::test]
    async fn test_get_discovery_properties_no_properties() {
        let _ = env_logger::builder().is_test(true).try_init();
        let discovery_operator = create_discovery_operator("../test/yaml/config-a.yaml");
        let mock_kube_interface: Arc<dyn k8s::KubeInterface> = Arc::new(MockKubeInterface::new());

        // properties should be empty if not specified
        assert!(discovery_operator
            .get_discovery_properties(mock_kube_interface, &None)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn test_get_discovery_properties_empty_property_list() {
        let _ = env_logger::builder().is_test(true).try_init();
        let discovery_operator = create_discovery_operator("../test/yaml/config-a.yaml");
        let mock_kube_interface: Arc<dyn k8s::KubeInterface> = Arc::new(MockKubeInterface::new());
        let properties = Vec::<DiscoveryProperty>::new();

        // properties should be empty if property list is empty
        assert!(discovery_operator
            .get_discovery_properties(mock_kube_interface, &Some(properties))
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_no_value() {
        let _ = env_logger::builder().is_test(true).try_init();
        let discovery_operator = create_discovery_operator("../test/yaml/config-a.yaml");
        let mock_kube_interface: Arc<dyn k8s::KubeInterface> = Arc::new(MockKubeInterface::new());
        let property_name_1 = "property_name_1".to_string();
        let property_name_2 = "".to_string(); // allow empty property name
        let properties = vec![
            DiscoveryProperty {
                name: property_name_1.clone(),
                value: None,
                value_from: None,
            },
            DiscoveryProperty {
                name: property_name_2.clone(),
                value: None,
                value_from: None,
            },
        ];
        let expected_result = HashMap::from([
            (property_name_1, ByteData { vec: None }),
            (property_name_2, ByteData { vec: None }),
        ]);

        // properties should only contain (name, None) if no value specified
        let result = discovery_operator
            .get_discovery_properties(mock_kube_interface.clone(), &Some(properties))
            .await
            .unwrap();
        assert_eq!(result, expected_result);
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_with_value() {
        let _ = env_logger::builder().is_test(true).try_init();
        let discovery_operator = create_discovery_operator("../test/yaml/config-a.yaml");
        let mock_kube_interface: Arc<dyn k8s::KubeInterface> = Arc::new(MockKubeInterface::new());
        let property_name_1 = "property_name_1".to_string();
        let property_name_2 = "".to_string(); // allow empty property name
        let property_value_1 = "property_value_1".to_string();
        let property_value_2 = "property_value_2".to_string();
        let properties = vec![
            DiscoveryProperty {
                name: property_name_1.clone(),
                value: Some(property_value_1.clone()),
                value_from: None,
            },
            DiscoveryProperty {
                name: property_name_2.clone(),
                value: Some(property_value_2.clone()),
                value_from: None,
            },
        ];
        let expected_result = HashMap::from([
            (
                property_name_1,
                ByteData {
                    vec: Some(property_value_1.into()),
                },
            ),
            (
                property_name_2,
                ByteData {
                    vec: Some(property_value_2.into()),
                },
            ),
        ]);

        // properties should contains (name, value) if specified
        let result = discovery_operator
            .get_discovery_properties(mock_kube_interface, &Some(properties))
            .await
            .unwrap();
        assert_eq!(result, expected_result);
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_secret_no_secret_found() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let secret_name = "secret_1";
        let key_in_secret = "key_in_secret";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_secret.to_string(),
            name: secret_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(false),
        };

        let mut mock_kube_client = MockKubeClient::new();
        mock_kube_client
            .expect_get_secret()
            .times(1)
            .withf(move |name: &str, namespace: &str| {
                namespace == namespace_name && name == secret_name
            })
            .returning(move |_, _| Ok(None));

        // get_discovery_property_value_from_secret should return error if secret not found
        let result = get_discovery_property_value_from_secret(&mock_kube_client, &selector).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_secret_no_secret_found_optional() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let secret_name = "secret_1";
        let key_in_secret = "key_in_secret";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_secret.to_string(),
            name: secret_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(true),
        };

        let mut mock_kube_client = MockKubeClient::new();
        mock_kube_client
            .expect_get_secret()
            .times(1)
            .withf(move |name: &str, namespace: &str| {
                namespace == namespace_name && name == secret_name
            })
            .returning(move |_, _| Ok(None));

        // get_discovery_property_value_from_secret for an optional key should return None if secret not found
        let result = get_discovery_property_value_from_secret(&mock_kube_client, &selector).await;
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_secret_no_key() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let secret_name = "secret_1";
        let key_in_secret = "key_in_secret";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_secret.to_string(),
            name: secret_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(false),
        };

        let mut mock_kube_client = MockKubeClient::new();
        mock_kube_client
            .expect_get_secret()
            .times(1)
            .withf(move |name: &str, namespace: &str| {
                namespace == namespace_name && name == secret_name
            })
            .returning(move |_, _| Ok(Some(Secret::default())));

        // get_discovery_property_value_from_secret should return error if key in secret not found
        assert!(
            get_discovery_property_value_from_secret(&mock_kube_client, &selector,)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_secret_no_key_optional() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let secret_name = "secret_1";
        let key_in_secret = "key_in_config_map";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_secret.to_string(),
            name: secret_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(true),
        };

        let mut mock_kube_client = MockKubeClient::new();
        mock_kube_client
            .expect_get_secret()
            .times(1)
            .withf(move |name: &str, namespace: &str| {
                namespace == namespace_name && name == secret_name
            })
            .returning(move |_, _| Ok(Some(Secret::default())));

        // get_discovery_property_value_from_secret for an optional key should return None if key in secret not found
        let result = get_discovery_property_value_from_secret(&mock_kube_client, &selector).await;
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_secret_no_value() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let secret_name = "secret_1";
        let key_in_secret = "key_in_secret";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_secret.to_string(),
            name: secret_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(false),
        };

        let mut mock_kube_client = MockKubeClient::new();
        mock_kube_client
            .expect_get_secret()
            .times(1)
            .withf(move |name: &str, namespace: &str| {
                namespace == namespace_name && name == secret_name
            })
            .returning(move |_, _| {
                let secret = Secret {
                    data: Some(BTreeMap::new()),
                    ..Default::default()
                };
                Ok(Some(secret))
            });

        // get_discovery_property_value_from_secret should return error if no value in secret
        let result = get_discovery_property_value_from_secret(&mock_kube_client, &selector).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_secret_no_value_optional() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let secret_name = "secret_1";
        let key_in_secret = "key_in_config_map";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_secret.to_string(),
            name: secret_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(true),
        };

        let mut mock_kube_client = MockKubeClient::new();
        mock_kube_client
            .expect_get_secret()
            .times(1)
            .withf(move |name: &str, namespace: &str| {
                namespace == namespace_name && name == secret_name
            })
            .returning(move |_, _| {
                let secret = Secret {
                    data: Some(BTreeMap::new()),
                    ..Default::default()
                };
                Ok(Some(secret))
            });

        // get_discovery_property_value_from_secret for an optional key should return None if key in secret not found
        let result = get_discovery_property_value_from_secret(&mock_kube_client, &selector).await;
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_secret_data_value() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let secret_name = "secret_1";
        let key_in_secret = "key_in_secret";
        let value_in_secret = "value_in_secret";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_secret.to_string(),
            name: secret_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(false),
        };

        let mut mock_kube_client = MockKubeClient::new();
        mock_kube_client
            .expect_get_secret()
            .times(1)
            .withf(move |name: &str, namespace: &str| {
                namespace == namespace_name && name == secret_name
            })
            .returning(move |_, _| {
                let data = BTreeMap::from([(
                    key_in_secret.to_string(),
                    ByteString(value_in_secret.into()),
                )]);
                let secret = Secret {
                    data: Some(data),
                    ..Default::default()
                };
                Ok(Some(secret))
            });

        let expected_result = ByteData {
            vec: Some(value_in_secret.into()),
        };

        // get_discovery_property_value_from_secret should return correct value if data value in secret
        let result = get_discovery_property_value_from_secret(&mock_kube_client, &selector).await;
        assert_eq!(result.unwrap().unwrap(), expected_result);
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_config_map_no_config_map_found() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let config_map_name = "config_map_1";
        let key_in_config_map = "key_in_config_map";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_config_map.to_string(),
            name: config_map_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(false),
        };

        let mut mock_kube_client = MockKubeClient::new();
        mock_kube_client
            .expect_get_config_map()
            .times(1)
            .withf(move |name: &str, namespace: &str| {
                namespace == namespace_name && name == config_map_name
            })
            .returning(move |_, _| Ok(None));

        // get_discovery_property_value_from_config_map should return error if configMap not found
        let result =
            get_discovery_property_value_from_config_map(&mock_kube_client, &selector).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_config_map_no_config_map_found_optional() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let config_map_name = "config_map_1";
        let key_in_config_map = "key_in_config_map";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_config_map.to_string(),
            name: config_map_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(true),
        };

        let mut mock_kube_client = MockKubeClient::new();
        mock_kube_client
            .expect_get_config_map()
            .times(1)
            .withf(move |name: &str, namespace: &str| {
                namespace == namespace_name && name == config_map_name
            })
            .returning(move |_, _| Ok(None));

        // get_discovery_property_value_from_config_map for an optional key should return None if configMap not found
        let result =
            get_discovery_property_value_from_config_map(&mock_kube_client, &selector).await;
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_config_map_no_key() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let config_map_name = "config_map_1";
        let key_in_config_map = "key_in_config_map";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_config_map.to_string(),
            name: config_map_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(false),
        };

        let mut mock_kube_client = MockKubeClient::new();
        mock_kube_client
            .expect_get_config_map()
            .times(1)
            .withf(move |name: &str, namespace: &str| {
                namespace == namespace_name && name == config_map_name
            })
            .returning(move |_, _| Ok(Some(ConfigMap::default())));

        // get_discovery_property_value_from_config_map should return error if key in configMap not found
        let result =
            get_discovery_property_value_from_config_map(&mock_kube_client, &selector).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_config_map_no_key_optional() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let config_map_name = "config_map_1";
        let key_in_config_map = "key_in_config_map";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_config_map.to_string(),
            name: config_map_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(true),
        };

        let mut mock_kube_client = MockKubeClient::new();
        mock_kube_client
            .expect_get_config_map()
            .times(1)
            .withf(move |name: &str, namespace: &str| {
                namespace == namespace_name && name == config_map_name
            })
            .returning(move |_, _| Ok(Some(ConfigMap::default())));

        // get_discovery_property_value_from_config_map for an optional key should return None if key in configMap not found
        let result =
            get_discovery_property_value_from_config_map(&mock_kube_client, &selector).await;
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_config_map_no_value() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let config_map_name = "config_map_1";
        let key_in_config_map = "key_in_config_map";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_config_map.to_string(),
            name: config_map_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(false),
        };

        let mut mock_kube_client = MockKubeClient::new();
        mock_kube_client
            .expect_get_config_map()
            .times(1)
            .withf(move |name: &str, namespace: &str| {
                namespace == namespace_name && name == config_map_name
            })
            .returning(move |_, _| {
                let config_map = ConfigMap {
                    data: Some(BTreeMap::new()),
                    binary_data: Some(BTreeMap::new()),
                    ..Default::default()
                };
                Ok(Some(config_map))
            });

        // get_discovery_property_value_from_config_map should return error if no value in configMap
        let result =
            get_discovery_property_value_from_config_map(&mock_kube_client, &selector).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_config_map_no_value_optional() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let config_map_name = "config_map_1";
        let key_in_config_map = "key_in_config_map";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_config_map.to_string(),
            name: config_map_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(true),
        };

        let mut mock_kube_client = MockKubeClient::new();
        mock_kube_client
            .expect_get_config_map()
            .times(1)
            .withf(move |name: &str, namespace: &str| {
                namespace == namespace_name && name == config_map_name
            })
            .returning(move |_, _| {
                let config_map = ConfigMap {
                    data: Some(BTreeMap::new()),
                    binary_data: Some(BTreeMap::new()),
                    ..Default::default()
                };
                Ok(Some(config_map))
            });

        // get_discovery_property_value_from_config_map for an optional key should return None if key in configMap not found
        let result =
            get_discovery_property_value_from_config_map(&mock_kube_client, &selector).await;
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_config_map_data_value() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let config_map_name = "config_map_1";
        let key_in_config_map = "key_in_config_map";
        let value_in_config_map = "value_in_config_map";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_config_map.to_string(),
            name: config_map_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(false),
        };

        let mut mock_kube_client = MockKubeClient::new();
        mock_kube_client
            .expect_get_config_map()
            .times(1)
            .withf(move |name: &str, namespace: &str| {
                namespace == namespace_name && name == config_map_name
            })
            .returning(move |_, _| {
                let data = BTreeMap::from([(
                    key_in_config_map.to_string(),
                    value_in_config_map.to_string(),
                )]);
                let config_map = ConfigMap {
                    data: Some(data),
                    binary_data: Some(BTreeMap::new()),
                    ..Default::default()
                };
                Ok(Some(config_map))
            });

        let expected_result = ByteData {
            vec: Some(value_in_config_map.into()),
        };

        // get_discovery_property_value_from_config_map should return correct value if data value in configMap
        let result =
            get_discovery_property_value_from_config_map(&mock_kube_client, &selector).await;
        assert_eq!(result.unwrap().unwrap(), expected_result);
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_config_map_binary_data_value() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let config_map_name = "config_map_1";
        let key_in_config_map = "key_in_config_map";
        let value_in_config_map = "value_in_config_map";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_config_map.to_string(),
            name: config_map_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(false),
        };

        let mut mock_kube_client = MockKubeClient::new();
        mock_kube_client
            .expect_get_config_map()
            .times(1)
            .withf(move |name: &str, namespace: &str| {
                namespace == namespace_name && name == config_map_name
            })
            .returning(move |_, _| {
                let binary_data = BTreeMap::from([(
                    key_in_config_map.to_string(),
                    ByteString(value_in_config_map.into()),
                )]);
                let config_map = ConfigMap {
                    data: Some(BTreeMap::new()),
                    binary_data: Some(binary_data),
                    ..Default::default()
                };
                Ok(Some(config_map))
            });

        let expected_result = ByteData {
            vec: Some(value_in_config_map.into()),
        };

        // get_discovery_property_value_from_config_map should return correct value if binary data value in configMap
        let result =
            get_discovery_property_value_from_config_map(&mock_kube_client, &selector).await;
        assert_eq!(result.unwrap().unwrap(), expected_result);
    }

    #[tokio::test]
    async fn test_get_discovery_properties_value_from_config_map_data_and_binary_data_value() {
        let _ = env_logger::builder().is_test(true).try_init();
        let namespace_name = "namespace_name";
        let config_map_name = "config_map_1";
        let key_in_config_map = "key_in_config_map";
        let value_in_config_map = "value_in_config_map";
        let binary_value_in_config_map = "binary_value_in_config_map";

        let selector = DiscoveryPropertyKeySelector {
            key: key_in_config_map.to_string(),
            name: config_map_name.to_string(),
            namespace: namespace_name.to_string(),
            optional: Some(false),
        };

        let mut mock_kube_client = MockKubeClient::new();
        mock_kube_client
            .expect_get_config_map()
            .times(1)
            .withf(move |name: &str, namespace: &str| {
                namespace == namespace_name && name == config_map_name
            })
            .returning(move |_, _| {
                let data = BTreeMap::from([(
                    key_in_config_map.to_string(),
                    value_in_config_map.to_string(),
                )]);
                let binary_data = BTreeMap::from([(
                    key_in_config_map.to_string(),
                    ByteString(binary_value_in_config_map.into()),
                )]);
                let config_map = ConfigMap {
                    data: Some(data),
                    binary_data: Some(binary_data),
                    ..Default::default()
                };
                Ok(Some(config_map))
            });

        let expected_result = ByteData {
            vec: Some(value_in_config_map.into()),
        };

        // get_discovery_property_value_from_config_map should return value from data if both data and binary data value exist
        let result =
            get_discovery_property_value_from_config_map(&mock_kube_client, &selector).await;
        assert_eq!(result.unwrap().unwrap(), expected_result);
    }

    #[tokio::test]
    async fn test_try_delete_instance() {
        let _ = env_logger::builder().is_test(true).try_init();
        // should do nothing for non existing instance
        let mut kube_interface = MockKubeInterface::new();
        kube_interface
            .expect_find_instance()
            .with(eq("foo"), eq("bar"))
            .returning(move |_, _| Err(anyhow::format_err!("Not Found")));
        try_delete_instance(&kube_interface, "foo", "bar", "node-a".to_string())
            .await
            .unwrap();

        // should delete instance with already empty node list
        let mut kube_interface = MockKubeInterface::new();
        let instance = get_test_instance(vec![]);
        kube_interface
            .expect_find_instance()
            .with(eq("foo"), eq("bar"))
            .returning(move |_, _| Ok(instance.clone()));
        kube_interface
            .expect_delete_instance()
            .with(eq("foo"), eq("bar"))
            .returning(move |_, _| Ok(()));
        try_delete_instance(&kube_interface, "foo", "bar", "node-a".to_string())
            .await
            .unwrap();

        // should delete instance with then empty node list
        let mut kube_interface = MockKubeInterface::new();
        let instance = get_test_instance(vec!["node-a"]);
        kube_interface
            .expect_find_instance()
            .with(eq("foo"), eq("bar"))
            .returning(move |_, _| Ok(instance.clone()));
        kube_interface
            .expect_delete_instance()
            .with(eq("foo"), eq("bar"))
            .returning(move |_, _| Ok(()));
        try_delete_instance(&kube_interface, "foo", "bar", "node-a".to_string())
            .await
            .unwrap();

        // should update instance with non empty node list
        let mut kube_interface = MockKubeInterface::new();
        let instance = get_test_instance(vec!["node-a", "node-b"]);
        kube_interface
            .expect_find_instance()
            .with(eq("foo"), eq("bar"))
            .returning(move |_, _| Ok(instance.clone()));
        kube_interface
            .expect_update_instance()
            .times(1)
            .withf(move |instance, name, namespace| {
                name == "foo" && namespace == "bar" && instance.nodes == vec!["node-b"]
            })
            .returning(move |_, _, _| Ok(()));
        try_delete_instance(&kube_interface, "foo", "bar", "node-a".to_string())
            .await
            .unwrap();
    }
}
