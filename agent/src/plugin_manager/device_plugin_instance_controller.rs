use std::collections::HashSet;
use std::fmt::Display;
use std::str::FromStr;
use std::{collections::HashMap, sync::Arc, time::Duration};

use akri_shared::{akri::instance::Instance, k8s::api::IntoApi};
use anyhow::Context;
use async_trait::async_trait;
use futures::StreamExt;
use itertools::Itertools;
use kube::api::{Patch, PatchParams};
use kube::core::{NotUsed, Object, ObjectMeta, TypeMeta};
use kube::{Resource, ResourceExt};
use kube_runtime::controller::Action;
use kube_runtime::reflector::Store;
use kube_runtime::Controller;
use thiserror::Error;
use tokio::sync::{watch, Mutex, RwLock};
use tokio::task::JoinHandle;
use tonic::Request;

use crate::device_manager::{cdi, DeviceManager};
use crate::plugin_manager::v1beta1::ContainerAllocateResponse;
use crate::util::stopper::Stopper;

use super::device_plugin_runner::{
    serve_and_register_plugin, DeviceUsageStream, InternalDevicePlugin,
};
use super::v1beta1::{AllocateRequest, AllocateResponse, ListAndWatchResponse};

pub const DP_SLOT_PREFIX: &str = "akri.sh/";

#[derive(Error, Debug)]
pub enum DevicePluginError {
    #[error("Slot already in use")]
    SlotInUse,

    #[error("No slots left for device")]
    NoSlot,

    #[error("Device usage parse error")]
    UsageParseError,

    #[error("Unknown device: {0}")]
    UnknownDevice(String),

    #[error(transparent)]
    RunnerError(#[from] super::device_plugin_runner::RunnerError),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Clone, PartialEq)]
enum DeviceUsage {
    Unused,
    Node(String),
    Configuration { vdev: String, node: String },
}

impl DeviceUsage {
    fn is_owned_by(&self, node: &str) -> bool {
        match self {
            Self::Node(n) if n == node => true,
            Self::Configuration { node: n, .. } if n == node => true,
            _ => false,
        }
    }
}

impl Display for DeviceUsage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceUsage::Unused => write!(f, ""),
            DeviceUsage::Node(node) => write!(f, "{}", node),
            DeviceUsage::Configuration { vdev, node } => write!(f, "C:{}:{}", vdev, node),
        }
    }
}

impl FromStr for DeviceUsage {
    type Err = DevicePluginError;
    fn from_str(val: &str) -> Result<Self, DevicePluginError> {
        if val.is_empty() {
            Ok(Self::Unused)
        } else {
            match val.split(':').collect_vec()[..] {
                ["C", vdev, node] => Ok(Self::Configuration {
                    vdev: vdev.to_owned(),
                    node: node.to_owned(),
                }),
                [node] => Ok(Self::Node(node.to_owned())),
                _ => Err(DevicePluginError::UsageParseError),
            }
        }
    }
}

fn parse_slot_id(st: &str) -> Result<usize, DevicePluginError> {
    usize::from_str(
        st.rsplit_once('-')
            .ok_or(DevicePluginError::UsageParseError)?
            .1,
    )
    .or(Err(DevicePluginError::UsageParseError))
}

fn construct_slots_map(
    slots: &HashMap<String, String>,
) -> Result<HashMap<usize, DeviceUsage>, DevicePluginError> {
    slots
        .iter()
        .map(|(k, v)| Ok((parse_slot_id(k)?, DeviceUsage::from_str(v)?)))
        .try_collect()
}

fn construct_slots_vec(
    slots: &HashMap<String, String>,
    capacity: usize,
) -> Result<Vec<DeviceUsage>, DevicePluginError> {
    let mut out_vec = vec![DeviceUsage::Unused; capacity];
    for (k, v) in slots.iter() {
        let index = parse_slot_id(k)?;
        if index >= capacity {
            return Err(DevicePluginError::UsageParseError);
        }
        out_vec[index] = DeviceUsage::from_str(v)?;
    }
    Ok(out_vec)
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct PartialInstanceSlotUsage {
    device_usage: HashMap<String, String>,
}

struct InstanceDevicePlugin {
    device: cdi::Device,
    slots_status: Mutex<watch::Sender<Vec<DeviceUsage>>>,
    node_name: String,
    instance_name: String,
    instance_namespace: String,
    kube_client: Arc<dyn IntoApi<Instance>>,
    stopper: Stopper,
}

impl InstanceDevicePlugin {
    fn new(
        node_name: String,
        plugin_name: String,
        namespace: String,
        device: cdi::Device,
        slots: &HashMap<String, String>,
        capacity: usize,
        client: Arc<dyn IntoApi<Instance>>,
    ) -> Result<Self, DevicePluginError> {
        let (slots_status, _) = watch::channel(construct_slots_vec(slots, capacity)?);
        Ok(Self {
            device,
            slots_status: Mutex::new(slots_status),
            node_name,
            instance_name: plugin_name,
            kube_client: client,
            stopper: Stopper::new(),
            instance_namespace: namespace,
        })
    }

    async fn update_slots(&self, slots: &HashMap<String, String>) -> Result<(), DevicePluginError> {
        let my_slots = self.slots_status.lock().await;
        let new_slots = construct_slots_map(slots)?;
        my_slots.send_if_modified(|current| {
            let mut modified = false;
            for (k, v) in new_slots.iter() {
                if current[*k] != *v {
                    v.clone_into(&mut current[*k]);
                    modified = true;
                }
            }
            modified
        });
        Ok(())
    }

    async fn claim_slot(
        &self,
        id: Option<usize>,
        wanted_state: DeviceUsage,
    ) -> Result<usize, DevicePluginError> {
        if wanted_state == DeviceUsage::Unused {
            return Err(anyhow::anyhow!("Should never happen").into());
        }
        let slots_status = self.slots_status.lock().await;
        let id = match id {
            Some(id) => match &slots_status.borrow()[id] {
                DeviceUsage::Unused => id,
                // The kubelet asks for the same slot, it knows best
                d if *d == wanted_state => id,
                _ => {
                    trace!("Trying to claim already used slot");
                    return Err(DevicePluginError::SlotInUse);
                }
            },
            None => slots_status
                .borrow()
                .iter()
                .position(|v| *v == DeviceUsage::Unused)
                .ok_or(DevicePluginError::NoSlot)?,
        };
        slots_status.send_modify(|slots| {
            slots[id] = wanted_state;
        });
        let device_usage = slots_status
            .borrow()
            .iter()
            .enumerate()
            .filter_map(|(i, v)| match v {
                v if v.is_owned_by(&self.node_name) => {
                    Some((format!("{}-{}", self.instance_name, i), v.to_string()))
                }
                _ => None,
            })
            .collect();
        let api = self.kube_client.namespaced(&self.instance_namespace);
        let patch = Patch::Apply(
            serde_json::to_value(Object {
                types: Some(TypeMeta {
                    api_version: Instance::api_version(&()).to_string(),
                    kind: Instance::kind(&()).to_string(),
                }),
                status: None::<NotUsed>,
                spec: PartialInstanceSlotUsage { device_usage },
                metadata: ObjectMeta {
                    name: Some(self.instance_name.to_owned()),
                    ..Default::default()
                },
            })
            .context("Could not create instance patch")?,
        );
        api.raw_patch(
            &self.instance_name,
            &patch,
            &PatchParams::apply(&format!("dp-{}", &self.node_name)),
        )
        .await
        .map_err(|e| match e {
            kube::Error::Api(ae) => match ae.code {
                409 => {
                    trace!("Conflict on apply {:?}", ae);
                    DevicePluginError::SlotInUse
                }
                _ => DevicePluginError::Other(ae.into()),
            },
            e => DevicePluginError::Other(e.into()),
        })?;
        Ok(id)
    }

    async fn free_slot(&self, id: usize) -> Result<(), DevicePluginError> {
        let slots_status = self.slots_status.lock().await;
        slots_status.send_if_modified(|slots| {
            if id >= slots.len() {
                // We try to free a slot that doesn't exists, probably already freed
                false
            } else {
                slots[id] = DeviceUsage::Unused;
                true
            }
        });
        let device_usage = slots_status
            .borrow()
            .iter()
            .enumerate()
            .filter_map(|(i, v)| match v {
                v if v.is_owned_by(&self.node_name) => {
                    Some((format!("{}-{}", self.instance_name, i), v.to_string()))
                }
                _ => None,
            })
            .collect();
        let api = self.kube_client.namespaced(&self.instance_namespace);
        let patch = Patch::Apply(
            serde_json::to_value(Object {
                types: Some(TypeMeta {
                    api_version: Instance::api_version(&()).to_string(),
                    kind: Instance::kind(&()).to_string(),
                }),
                status: None::<NotUsed>,
                spec: PartialInstanceSlotUsage { device_usage },
                metadata: ObjectMeta {
                    name: Some(self.instance_name.to_owned()),
                    ..Default::default()
                },
            })
            .context("Could not create instance patch")?,
        );
        api.raw_patch(
            &self.instance_name,
            &patch,
            &PatchParams::apply(&format!("dp-{}", &self.node_name)),
        )
        .await
        .map_err(|e| match e {
            kube::Error::Api(ae) => match ae.code {
                409 => DevicePluginError::SlotInUse,
                _ => DevicePluginError::Other(ae.into()),
            },
            e => DevicePluginError::Other(e.into()),
        })?;
        Ok(())
    }
}

fn instance_device_usage_to_device(
    device_name: &str,
    node_name: &str,
    devices: Vec<DeviceUsage>,
) -> Result<ListAndWatchResponse, tonic::Status> {
    let devices = devices
        .into_iter()
        .enumerate()
        .map(|(id, dev)| super::v1beta1::Device {
            id: format!("{}-{}", device_name, id),
            health: match dev {
                DeviceUsage::Unused => "Healthy",
                DeviceUsage::Configuration { .. } => "Unhealthy",
                DeviceUsage::Node(n) => match n == node_name {
                    true => "Healthy",
                    false => "Unhealthy",
                },
            }
            .to_string(),
            topology: None,
        })
        .collect();
    trace!("Sending devices to kubelet: {:?}", devices);
    Ok(ListAndWatchResponse { devices })
}

#[async_trait]
impl InternalDevicePlugin for InstanceDevicePlugin {
    type DeviceStore = Vec<DeviceUsage>;

    fn get_name(&self) -> String {
        self.instance_name.clone()
    }

    fn stop(&self) {
        trace!("stopping device plugin");
        self.stopper.stop()
    }

    async fn stopped(&self) {
        self.stopper.stopped().await;
        trace!("plugin {} stopped", self.instance_name);
    }

    async fn list_and_watch(
        &self,
    ) -> Result<tonic::Response<DeviceUsageStream<Self::DeviceStore>>, tonic::Status> {
        info!(
            "list_and_watch - kubelet called list_and_watch for instance {}",
            self.instance_name
        );
        let device_name = self.instance_name.clone();
        let node_name = self.node_name.clone();
        let receiver = self.slots_status.lock().await.subscribe();
        let receiver_stream = tokio_stream::wrappers::WatchStream::new(receiver);

        Ok(tonic::Response::new(DeviceUsageStream {
            device_usage_to_device: instance_device_usage_to_device,
            input_stream: self.stopper.make_abortable(receiver_stream),
            device_name,
            node_name,
        }))
    }

    /// Kubelet calls allocate during pod creation.
    /// This means kubelet is trying to reserve a usage slot (virtual Device) of the Instance for this node.
    /// Returns error if cannot reserve that slot.
    async fn allocate(
        &self,
        requests: Request<AllocateRequest>,
    ) -> Result<tonic::Response<AllocateResponse>, tonic::Status> {
        info!(
            "allocate - kubelet called allocate for Instance {}",
            self.instance_name
        );
        let mut container_responses: Vec<super::v1beta1::ContainerAllocateResponse> = Vec::new();
        let reqs = requests.into_inner().container_requests;
        for allocate_request in reqs {
            let devices = allocate_request.devices_i_ds;
            for device in devices {
                let (_, id) = device
                    .rsplit_once('-')
                    .ok_or(tonic::Status::unknown("Invalid device id"))?;
                let id = id
                    .parse::<usize>()
                    .or(Err(tonic::Status::unknown("Invalid device id")))?;
                self.claim_slot(Some(id), DeviceUsage::Node(self.node_name.to_owned()))
                    .await
                    .map_err(|e| {
                        error!("Unable to claim slot: {:?}", e);
                        tonic::Status::unknown("Unable to claim slot")
                    })?;
            }
            container_responses.push(cdi_device_to_car(&self.device));
        }
        Ok(tonic::Response::new(AllocateResponse {
            container_responses,
        }))
    }
}

fn cdi_device_to_car(device: &cdi::Device) -> ContainerAllocateResponse {
    // Device name is in the format akri.sh/c<config_name>-<instance-hash>
    // Append deterministic instance hash to broker envs to avoid conflicts
    let instance_hash = device
        .name
        .split('-')
        .last()
        .unwrap_or_default()
        .to_uppercase();

    // Mount all device environment variables with and without a suffix of the
    // instance hash. Envs without a suffix could be undeterministically
    // overridden by other allocated instances discovered by the same Discovery
    // Handler. Unsuffixed envs should only be referenced if they are
    // additional Configuration.broker_properties or if only one instance of a
    // DH is allocated to the broker.
    let envs = device
        .container_edits
        .env
        .iter()
        .map(|e| match e.split_once('=') {
            Some((k, v)) => (k.to_string(), v.to_string()),
            None => (e.to_string(), "".to_string()),
        });

    let suffixed_envs = envs
        .clone()
        .map(|(k, v)| (format!("{}_{}", k, instance_hash), v));

    ContainerAllocateResponse {
        envs: envs.chain(suffixed_envs).collect(),
        mounts: device
            .container_edits
            .mounts
            .iter()
            .map(|m| super::v1beta1::Mount {
                container_path: m.container_path.clone(),
                host_path: m.host_path.clone(),
                read_only: m.options.contains(&"ro".to_string()),
            })
            .collect(),
        devices: device
            .container_edits
            .device_nodes
            .iter()
            .map(|d| super::v1beta1::DeviceSpec {
                container_path: d.path.clone(),
                host_path: d.host_path.clone().unwrap_or(d.path.clone()),
                permissions: d.permissions.clone().unwrap_or_default(),
            })
            .collect(),
        annotations: device.annotations.clone(),
    }
}

#[derive(Clone, Debug, PartialEq)]
enum ConfigurationSlot {
    DeviceFree(String),
    DeviceUsed { device: String, slot_id: usize },
}

struct ConfigurationDevicePlugin {
    instances: RwLock<HashMap<String, Arc<InstanceDevicePlugin>>>,
    slots: Arc<RwLock<watch::Sender<HashMap<String, ConfigurationSlot>>>>,
    config_name: String,
    node_name: String,
    stopper: Stopper,
}

impl ConfigurationDevicePlugin {
    fn new(config_name: String, node_name: String) -> Self {
        let (slots, _) = watch::channel(Default::default());
        Self {
            instances: Default::default(),
            slots: Arc::new(RwLock::new(slots)),
            config_name,
            node_name,
            stopper: Stopper::new(),
        }
    }
    async fn add_plugin(&self, name: String, plugin: Arc<InstanceDevicePlugin>) {
        self.instances
            .write()
            .await
            .insert(name.to_owned(), plugin.clone());
        let node_name = self.node_name.clone();
        let slots_ref = self.slots.clone();
        let config_name = self.config_name.clone();
        let instance_name = plugin.instance_name.clone();
        let mut receiver = plugin.slots_status.lock().await.subscribe();
        tokio::spawn(async move {
            loop {
                {
                    let (has_free, used_config_slots) = {
                        let values = receiver.borrow_and_update();
                        let has_free = values.contains(&DeviceUsage::Unused);
                        let used_config_slots: HashMap<String, ConfigurationSlot> = values
                            .iter()
                            .enumerate()
                            .filter_map(|(slot, du)| match du {
                                DeviceUsage::Configuration { vdev, node } if *node == node_name => {
                                    Some((
                                        vdev.clone(),
                                        ConfigurationSlot::DeviceUsed {
                                            device: instance_name.clone(),
                                            slot_id: slot,
                                        },
                                    ))
                                }
                                _ => None,
                            })
                            .collect();
                        (has_free, used_config_slots)
                    };
                    slots_ref.write().await.send_if_modified(|slots| {
                        let mut modified = false;
                        let mut free_slot_available = has_free;
                        // Check for slots to remove
                        for (slot, usage) in slots.clone().iter() {
                            let to_remove = match usage {
                                ConfigurationSlot::DeviceFree(d) if *d == instance_name => {
                                    if !free_slot_available {
                                        true
                                    } else {
                                        free_slot_available = false;
                                        false
                                    }
                                }
                                ConfigurationSlot::DeviceUsed { device, .. }
                                    if *device == instance_name =>
                                {
                                    used_config_slots.get(slot) != Some(usage)
                                }
                                _ => false,
                            };
                            if to_remove {
                                modified = true;
                                slots.remove(slot);
                            }
                        }
                        let cur_length = slots.len();
                        slots.extend(used_config_slots);
                        if free_slot_available {
                            let mut used_slots_ids = slots
                                .keys()
                                .map(|k| {
                                    let (_, id) = k.rsplit_once('-').unwrap();
                                    id.parse::<usize>().unwrap()
                                })
                                .sorted()
                                .rev()
                                .collect_vec();
                            let mut possible_slot = 0usize;
                            while used_slots_ids.pop() == Some(possible_slot) {
                                possible_slot += 1;
                            }
                            slots.insert(
                                format!("{}-{}", config_name, possible_slot),
                                ConfigurationSlot::DeviceFree(instance_name.clone()),
                            );
                        }
                        modified || cur_length != slots.len()
                    });
                }
                tokio::select! {
                    a = receiver.changed() => {
                        if a.is_err() {
                            break;
                        }
                    },
                }
            }
            slots_ref.write().await.send_modify(|slots| {
                // Only keep slots that are unrelated to the current plugin
                slots.retain(|_, v| match v {
                    ConfigurationSlot::DeviceFree(p) if *p == instance_name => false,
                    ConfigurationSlot::DeviceUsed { device, .. } if *device == instance_name => {
                        false
                    }
                    _ => true,
                })
            });
        });
    }
    async fn remove_plugin(&self, name: &str) -> bool {
        let mut instances = self.instances.write().await;
        instances.remove(name);
        instances.is_empty()
    }

    async fn free_slot(&self, id: usize) -> Result<(), DevicePluginError> {
        let slot_id = format!("{}-{}", self.config_name, id);
        let slot = self.slots.read().await.borrow().get(&slot_id).cloned();
        if let Some(ConfigurationSlot::DeviceUsed { device, slot_id }) = slot {
            if let Some(dp) = self.instances.read().await.get(&device) {
                dp.free_slot(slot_id).await?;
            } else {
                error!("Tried to free used slot for gone instance device plugin");
            }
        }
        Ok(())
    }
}

#[async_trait]
impl InternalDevicePlugin for ConfigurationDevicePlugin {
    type DeviceStore = HashMap<String, ConfigurationSlot>;

    fn get_name(&self) -> String {
        self.config_name.clone()
    }

    async fn stopped(&self) {
        self.stopper.stopped().await;
        trace!("plugin {} stopped", self.config_name);
    }

    fn stop(&self) {
        self.stopper.stop()
    }

    async fn list_and_watch(
        &self,
    ) -> Result<tonic::Response<DeviceUsageStream<Self::DeviceStore>>, tonic::Status> {
        info!(
            "list_and_watch - kubelet called list_and_watch for Configuration {}",
            self.config_name
        );
        let device_name = self.config_name.clone();
        let node_name = self.node_name.clone();
        let receiver = self.slots.read().await.subscribe();
        let receiver_stream = tokio_stream::wrappers::WatchStream::new(receiver);

        Ok(tonic::Response::new(DeviceUsageStream {
            device_usage_to_device: config_device_usage_to_device,
            input_stream: self.stopper.make_abortable(receiver_stream),
            device_name,
            node_name,
        }))
    }

    /// Kubelet calls allocate during pod creation.
    /// This means kubelet is trying to reserve a usage slot (virtual Device) of the Instance for this node.
    /// Returns error if cannot reserve that slot.
    async fn allocate(
        &self,
        requests: Request<AllocateRequest>,
    ) -> Result<tonic::Response<AllocateResponse>, tonic::Status> {
        info!(
            "allocate - kubelet called allocate for Configuration {}",
            self.config_name
        );
        let mut container_responses: Vec<super::v1beta1::ContainerAllocateResponse> = Vec::new();
        let reqs = requests.into_inner().container_requests;
        for allocate_request in reqs {
            let devices = allocate_request.devices_i_ds;
            for device in devices {
                let dev = self
                    .slots
                    .read()
                    .await
                    .borrow()
                    .get(&device)
                    .ok_or(tonic::Status::unknown("Unable to claim slot"))?
                    .clone();
                if let ConfigurationSlot::DeviceFree(dev) = dev {
                    let dp = self
                        .instances
                        .read()
                        .await
                        .get(&dev)
                        .ok_or(tonic::Status::unknown("Invalid slot"))?
                        .clone();
                    container_responses.push(cdi_device_to_car(&dp.device));
                    dp.claim_slot(
                        None,
                        DeviceUsage::Configuration {
                            vdev: device.clone(),
                            node: self.node_name.clone(),
                        },
                    )
                    .await
                    .or(Err(tonic::Status::unknown("Unavailable slot")))?;
                } else {
                    return Err(tonic::Status::unknown("Unable to claim slot"));
                }
            }
        }
        Ok(tonic::Response::new(AllocateResponse {
            container_responses,
        }))
    }
}

fn config_device_usage_to_device(
    _device_name: &str,
    _node_name: &str,
    devices: HashMap<String, ConfigurationSlot>,
) -> Result<ListAndWatchResponse, tonic::Status> {
    Ok(ListAndWatchResponse {
        devices: devices
            .into_keys()
            .map(|id| super::v1beta1::Device {
                id,
                health: "Healthy".to_string(),
                topology: None,
            })
            .collect(),
    })
}

/// This module implements a controller for Instance resources that will ensure device plugins are correctly created with the correct health status

pub struct DevicePluginManager {
    instance_plugins: Mutex<HashMap<String, Arc<InstanceDevicePlugin>>>,
    configuration_plugins: Mutex<HashMap<String, Arc<ConfigurationDevicePlugin>>>,
    node_name: String,
    kube_client: Arc<dyn IntoApi<Instance>>,
    device_manager: Arc<dyn DeviceManager>,
    error_backoffs: std::sync::Mutex<HashMap<String, Duration>>,
}

const SUCCESS_REQUEUE: Duration = Duration::from_secs(600);

impl DevicePluginManager {
    pub fn new(
        node_name: String,
        kube_client: Arc<dyn IntoApi<Instance>>,
        device_manager: Arc<dyn DeviceManager>,
    ) -> Self {
        Self {
            instance_plugins: Mutex::new(HashMap::default()),
            configuration_plugins: Mutex::new(HashMap::default()),
            node_name,
            kube_client,
            device_manager,
            error_backoffs: std::sync::Mutex::new(HashMap::default()),
        }
    }

    pub async fn free_slot(&self, device_id: String) -> Result<(), DevicePluginError> {
        let (plugin_name, slot_id) = device_id
            .rsplit_once('-')
            .ok_or(DevicePluginError::UsageParseError)?;
        let slot_id = slot_id
            .parse::<usize>()
            .map_err(|_| DevicePluginError::UsageParseError)?;
        {
            let plugin = self.instance_plugins.lock().await.get(plugin_name).cloned();
            if let Some(plugin) = plugin {
                return plugin.free_slot(slot_id).await;
            }
        }
        {
            let plugin = self
                .configuration_plugins
                .lock()
                .await
                .get(plugin_name)
                .cloned();
            if let Some(plugin) = plugin {
                return plugin.free_slot(slot_id).await;
            }
        }
        Err(DevicePluginError::NoSlot)
    }

    pub async fn get_used_slots(&self) -> HashSet<String> {
        let mut slots: HashSet<String> = Default::default();
        for (instance, plugin) in self.instance_plugins.lock().await.iter() {
            slots.extend(
                plugin
                    .slots_status
                    .lock()
                    .await
                    .borrow()
                    .iter()
                    .enumerate()
                    .filter_map(|(i, u)| match u {
                        DeviceUsage::Node(n) if *n == self.node_name => {
                            Some(format!("{}{}-{}", DP_SLOT_PREFIX, instance, i))
                        }
                        DeviceUsage::Configuration { vdev, node } if *node == self.node_name => {
                            Some(vdev.to_string())
                        }
                        _ => None,
                    }),
            );
        }
        slots
    }
}

pub fn start_dpm(dpm: Arc<DevicePluginManager>) -> (Store<Instance>, JoinHandle<()>) {
    let api = dpm.kube_client.all().as_inner();
    let controller = Controller::new(api, Default::default());
    let store = controller.store();
    let task = tokio::spawn(async {
        controller
            .run(reconcile, error_policy, dpm)
            .for_each(|_| futures::future::ready(()))
            .await
    });
    (store, task)
}

pub async fn reconcile(
    instance: Arc<Instance>,
    ctx: Arc<DevicePluginManager>,
) -> Result<Action, DevicePluginError> {
    trace!("Plugin Manager: Reconciling {}", instance.name_any());
    let api = ctx.kube_client.namespaced(&instance.namespace().unwrap());
    if !instance.spec.nodes.contains(&ctx.node_name)
        || instance.metadata.deletion_timestamp.is_some()
    {
        let mut cps = ctx.configuration_plugins.lock().await;
        if let Some(cp) = cps.get(&instance.spec.configuration_name) {
            if cp.remove_plugin(&instance.name_any()).await {
                cp.stop();
                cps.remove(&instance.spec.configuration_name);
            }
        }
        if let Some(plugin) = ctx
            .instance_plugins
            .lock()
            .await
            .remove(&instance.name_any())
        {
            plugin.stop();
        }
        api.remove_finalizer(&instance, &ctx.node_name)
            .await
            .map_err(|e| DevicePluginError::Other(e.into()))?;
    } else {
        let device = ctx.device_manager.get(&instance.spec.cdi_name).ok_or(
            DevicePluginError::UnknownDevice(instance.spec.cdi_name.to_owned()),
        )?;
        api.add_finalizer(&instance, &ctx.node_name)
            .await
            .map_err(|e| DevicePluginError::Other(e.into()))?;

        let instance_plugin = {
            let mut instance_plugins = ctx.instance_plugins.lock().await;
            match instance_plugins.get(&instance.name_any()) {
                None => {
                    let plugin = Arc::new(InstanceDevicePlugin::new(
                        ctx.node_name.to_owned(),
                        instance.name_any(),
                        instance.namespace().unwrap_or("default".to_string()),
                        device,
                        &instance.spec.device_usage,
                        instance.spec.capacity,
                        ctx.kube_client.clone(),
                    )?);
                    serve_and_register_plugin(plugin.clone()).await?;
                    instance_plugins.insert(instance.name_any(), plugin.clone());
                    plugin
                }
                Some(plugin) => {
                    // TODO: Add a way to handle a change in the instance's capacity.
                    plugin.update_slots(&instance.spec.device_usage).await?;
                    plugin.clone()
                }
            }
        };
        let configuration_plugin = {
            let mut configuration_plugins = ctx.configuration_plugins.lock().await;
            match configuration_plugins.get(&instance.spec.configuration_name) {
                None => {
                    let plugin = Arc::new(ConfigurationDevicePlugin::new(
                        instance.spec.configuration_name.to_owned(),
                        ctx.node_name.to_owned(),
                    ));
                    serve_and_register_plugin(plugin.clone()).await?;
                    configuration_plugins
                        .insert(instance.spec.configuration_name.to_owned(), plugin.clone());
                    plugin
                }
                Some(plugin) => plugin.clone(),
            }
        };
        configuration_plugin
            .add_plugin(instance.name_any(), instance_plugin)
            .await;
    }
    ctx.error_backoffs
        .lock()
        .unwrap()
        .remove(&instance.name_any());
    Ok(Action::requeue(SUCCESS_REQUEUE))
}

pub fn error_policy(
    dc: Arc<Instance>,
    error: &DevicePluginError,
    ctx: Arc<DevicePluginManager>,
) -> Action {
    let mut error_backoffs = ctx.error_backoffs.lock().unwrap();
    let previous_duration = error_backoffs
        .get(&dc.name_any())
        .cloned()
        .unwrap_or(Duration::from_millis(500));
    let next_duration = previous_duration * 2;
    warn!(
        "Error during reconciliation of Instance {:?}::{}, retrying in {}s: {:?}",
        dc.namespace(),
        dc.name_any(),
        next_duration.as_secs_f32(),
        error
    );
    error_backoffs.insert(dc.name_any(), next_duration);
    Action::requeue(next_duration)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use akri_shared::{
        akri::instance::InstanceSpec,
        k8s::api::{MockApi, MockIntoApi},
    };
    use tokio_stream::StreamExt;

    use crate::plugin_manager::v1beta1::ContainerAllocateRequest;

    use self::cdi::{ContainerEdit, Device};

    use super::*;

    #[test]
    fn test_device_usage() -> Result<(), DevicePluginError> {
        assert_eq!(
            DeviceUsage::from_str("node-a")?,
            DeviceUsage::Node("node-a".to_string())
        );
        assert_eq!(
            DeviceUsage::from_str("C:vdev1:node-a")?,
            DeviceUsage::Configuration {
                vdev: "vdev1".to_string(),
                node: "node-a".to_string()
            },
        );
        assert_eq!(DeviceUsage::from_str("")?, DeviceUsage::Unused,);
        assert!(DeviceUsage::from_str("C:node-a").is_err());

        Ok(())
    }

    #[test]
    fn test_parse_slot_id() -> Result<(), DevicePluginError> {
        assert_eq!(parse_slot_id("slot-1")?, 1);
        assert_eq!(parse_slot_id("my-other-slot-2")?, 2);
        assert!(parse_slot_id("not-a-slot-id").is_err());
        Ok(())
    }

    #[test]
    fn test_construct_slots_map() -> Result<(), DevicePluginError> {
        let slots = HashMap::from([
            ("slot-1".to_owned(), "node-a".to_owned()),
            ("slot-3".to_owned(), "C:vdev1:node-a".to_owned()),
        ]);
        assert_eq!(
            construct_slots_map(&slots)?,
            HashMap::from([
                (1, DeviceUsage::Node("node-a".to_owned())),
                (
                    3,
                    DeviceUsage::Configuration {
                        vdev: "vdev1".to_owned(),
                        node: "node-a".to_owned()
                    }
                )
            ])
        );
        Ok(())
    }

    #[test]
    fn test_construct_slots_vec() -> Result<(), DevicePluginError> {
        let slots = HashMap::from([
            ("slot-1".to_owned(), "node-a".to_owned()),
            ("slot-3".to_owned(), "C:vdev1:node-a".to_owned()),
        ]);
        assert_eq!(
            construct_slots_vec(&slots, 4)?,
            vec![
                DeviceUsage::Unused,
                DeviceUsage::Node("node-a".to_string()),
                DeviceUsage::Unused,
                DeviceUsage::Configuration {
                    vdev: "vdev1".to_owned(),
                    node: "node-a".to_owned()
                }
            ]
        );
        assert!(construct_slots_vec(&slots, 1).is_err());
        Ok(())
    }

    #[tokio::test]
    async fn test_instance_plugin_update_slots() {
        let plugin = InstanceDevicePlugin::new(
            "node-a".to_owned(),
            "my-device".to_owned(),
            "namespace-a".to_owned(),
            Device {
                name: "my-device".to_owned(),
                annotations: Default::default(),
                container_edits: ContainerEdit {
                    ..Default::default()
                },
            },
            &HashMap::new(),
            3,
            Arc::new(MockIntoApi::new()),
        )
        .unwrap();

        assert!(plugin
            .update_slots(&HashMap::from([("slot-1".to_owned(), "node-a".to_owned())]))
            .await
            .is_ok(),);

        assert_eq!(
            plugin.slots_status.lock().await.borrow()[1],
            DeviceUsage::Node("node-a".to_owned())
        );
    }

    #[tokio::test]
    async fn test_free_slot() {
        let dm = crate::device_manager::MockDeviceManager::new();
        let mut kube_client = MockIntoApi::new();
        kube_client.expect_namespaced().returning(|_| {
            let mut api = MockApi::new();
            api.expect_raw_patch()
                .with(
                    mockall::predicate::eq("instance-a"),
                    mockall::predicate::function(|a: &Patch<serde_json::Value>| match a {
                        Patch::Apply(v) => {
                            let su: Object<PartialInstanceSlotUsage, NotUsed> =
                                serde_json::from_value(v.clone()).unwrap();
                            error!("{:?}", su.spec.device_usage);
                            su.spec.device_usage.is_empty()
                        }
                        _ => false,
                    }),
                    mockall::predicate::always(),
                )
                .returning(|_, _, _| {
                    Ok(Instance {
                        metadata: Default::default(),
                        spec: InstanceSpec {
                            configuration_name: "config-a".to_owned(),
                            cdi_name: Default::default(),
                            capacity: 1,
                            broker_properties: Default::default(),
                            shared: false,
                            nodes: Default::default(),
                            device_usage: Default::default(),
                        },
                    })
                });
            Box::new(api)
        });
        let kube_client = Arc::new(kube_client);
        let dpm = DevicePluginManager::new("node-a".to_owned(), kube_client.clone(), Arc::new(dm));

        let stopper = Stopper::new();

        let (s, _) = watch::channel(vec![
            DeviceUsage::Configuration {
                vdev: "config-a-1".to_owned(),
                node: "node-a".to_owned(),
            },
            DeviceUsage::Node("node-b".to_owned()),
        ]);

        let instance_plugin = Arc::new(InstanceDevicePlugin {
            device: Device {
                name: "my-device".to_owned(),
                annotations: Default::default(),
                container_edits: ContainerEdit {
                    ..Default::default()
                },
            },
            slots_status: Mutex::new(s),
            node_name: "node-a".to_owned(),
            instance_name: "instance-a".to_owned(),
            instance_namespace: "namespace-a".to_owned(),
            kube_client,
            stopper: stopper.clone(),
        });
        dpm.instance_plugins
            .lock()
            .await
            .insert("instance-a".to_owned(), instance_plugin.clone());

        let (s, _) = watch::channel(HashMap::from([(
            "config-a-1".to_owned(),
            ConfigurationSlot::DeviceUsed {
                device: "instance-a".to_owned(),
                slot_id: 0,
            },
        )]));

        dpm.configuration_plugins.lock().await.insert(
            "config-a".to_owned(),
            Arc::new(ConfigurationDevicePlugin {
                instances: RwLock::new(HashMap::from([("instance-a".to_owned(), instance_plugin)])),
                slots: Arc::new(RwLock::new(s)),
                config_name: "config-a".to_owned(),
                node_name: "node-a".to_string(),
                stopper,
            }),
        );

        assert!(dpm.free_slot("config-b-2".to_owned()).await.is_err());
        assert!(dpm.free_slot("config-a-1".to_owned()).await.is_ok());
    }

    #[tokio::test]
    async fn test_get_used_slots() {
        let dm = crate::device_manager::MockDeviceManager::new();
        let kube_client = Arc::new(MockIntoApi::new());
        let stopper = Stopper::new();
        let dpm = DevicePluginManager::new("node-a".to_owned(), kube_client.clone(), Arc::new(dm));

        assert!(dpm.get_used_slots().await.is_empty());

        let (s, _) = watch::channel(vec![
            DeviceUsage::Configuration {
                vdev: "akri.sh/config-a-1".to_owned(),
                node: "node-a".to_owned(),
            },
            DeviceUsage::Node("node-a".to_owned()),
            DeviceUsage::Node("node-b".to_owned()),
            DeviceUsage::Unused,
        ]);
        let instance_plugin = Arc::new(InstanceDevicePlugin {
            device: Device {
                name: "my-device".to_owned(),
                annotations: Default::default(),
                container_edits: ContainerEdit {
                    ..Default::default()
                },
            },
            slots_status: Mutex::new(s),
            node_name: "node-a".to_owned(),
            instance_name: "instance-a".to_owned(),
            instance_namespace: "namespace-a".to_owned(),
            kube_client,
            stopper: stopper.clone(),
        });
        dpm.instance_plugins
            .lock()
            .await
            .insert("instance-a".to_owned(), instance_plugin);
        assert_eq!(
            dpm.get_used_slots().await,
            HashSet::from([
                "akri.sh/config-a-1".to_owned(),
                "akri.sh/instance-a-1".to_owned()
            ])
        );
    }

    #[tokio::test]
    async fn test_config_plugin_add_remove_plugin() {
        let kube_client = Arc::new(MockIntoApi::new());
        let stopper = Stopper::new();
        let (s, mut r) = watch::channel(vec![
            DeviceUsage::Configuration {
                vdev: "akri.sh/config-a-1".to_owned(),
                node: "node-a".to_owned(),
            },
            DeviceUsage::Node("node-a".to_owned()),
            DeviceUsage::Node("node-b".to_owned()),
            DeviceUsage::Unused,
        ]);
        let instance_plugin = Arc::new(InstanceDevicePlugin {
            device: Device {
                name: "my-device".to_owned(),
                annotations: Default::default(),
                container_edits: ContainerEdit {
                    ..Default::default()
                },
            },
            slots_status: Mutex::new(s),
            node_name: "node-a".to_owned(),
            instance_name: "instance-a".to_owned(),
            instance_namespace: "namespace-a".to_owned(),
            kube_client,
            stopper: stopper.clone(),
        });

        let config_plugin =
            ConfigurationDevicePlugin::new("config-a".to_owned(), "node-a".to_owned());
        config_plugin
            .add_plugin("instance-a".to_owned(), instance_plugin.clone())
            .await;

        assert_eq!(config_plugin.instances.read().await.len(), 1);

        tokio::time::sleep(Duration::from_millis(500)).await;

        assert_eq!(
            config_plugin.slots.read().await.borrow().clone(),
            HashMap::from([
                (
                    "akri.sh/config-a-1".to_owned(),
                    ConfigurationSlot::DeviceUsed {
                        device: "instance-a".to_owned(),
                        slot_id: 0
                    }
                ),
                (
                    "config-a-0".to_owned(),
                    ConfigurationSlot::DeviceFree("instance-a".to_owned())
                )
            ])
        );

        instance_plugin
            .slots_status
            .lock()
            .await
            .send_modify(|slots| slots[3] = DeviceUsage::Node("node-a".to_string()));
        drop(instance_plugin);

        tokio::time::sleep(Duration::from_millis(500)).await;
        r.borrow_and_update();

        assert_eq!(
            config_plugin.slots.read().await.borrow().clone(),
            HashMap::from([(
                "akri.sh/config-a-1".to_owned(),
                ConfigurationSlot::DeviceUsed {
                    device: "instance-a".to_owned(),
                    slot_id: 0
                }
            ),])
        );
        config_plugin.remove_plugin("instance-a").await;
        tokio::time::sleep(Duration::from_millis(500)).await;

        assert!(config_plugin.instances.read().await.is_empty());
        assert!(
            tokio::time::timeout(Duration::from_millis(500), r.changed())
                .await
                .unwrap()
                .is_err()
        );
        assert!(config_plugin.slots.read().await.borrow().is_empty());
    }

    #[tokio::test]
    async fn test_config_plugin_allocate() {
        let mut kube_client = MockIntoApi::new();
        kube_client.expect_namespaced().returning(|_| {
            let mut api = MockApi::new();
            api.expect_raw_patch().returning(|_, _, _| {
                Ok(Instance {
                    metadata: Default::default(),
                    spec: InstanceSpec {
                        configuration_name: "config-a".to_owned(),
                        cdi_name: Default::default(),
                        capacity: 1,
                        broker_properties: Default::default(),
                        shared: false,
                        nodes: Default::default(),
                        device_usage: Default::default(),
                    },
                })
            });
            Box::new(api)
        });
        let kube_client = Arc::new(kube_client);
        let stopper = Stopper::new();
        let (s, _) = watch::channel(vec![
            DeviceUsage::Configuration {
                vdev: "akri.sh/config-a-1".to_owned(),
                node: "node-a".to_owned(),
            },
            DeviceUsage::Node("node-a".to_owned()),
            DeviceUsage::Node("node-b".to_owned()),
            DeviceUsage::Unused,
        ]);
        let instance_plugin = Arc::new(InstanceDevicePlugin {
            device: Device {
                name: "my-device".to_owned(),
                annotations: Default::default(),
                container_edits: ContainerEdit {
                    ..Default::default()
                },
            },
            slots_status: Mutex::new(s),
            node_name: "node-a".to_owned(),
            instance_name: "instance-a".to_owned(),
            instance_namespace: "namespace-a".to_owned(),
            kube_client,
            stopper: stopper.clone(),
        });

        let config_plugin =
            ConfigurationDevicePlugin::new("config-a".to_owned(), "node-a".to_owned());
        config_plugin
            .add_plugin("instance-a".to_owned(), instance_plugin)
            .await;

        tokio::time::sleep(Duration::from_millis(500)).await;

        assert_eq!(
            config_plugin
                .allocate(Request::new(AllocateRequest {
                    container_requests: vec![ContainerAllocateRequest {
                        devices_i_ds: vec!["config-a-0".to_owned()],
                    }],
                }))
                .await
                .unwrap()
                .into_inner()
                .container_responses
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn test_instance_plugin_allocate() {
        let mut kube_client = MockIntoApi::new();
        kube_client.expect_namespaced().returning(|_| {
            let mut api = MockApi::new();
            api.expect_raw_patch()
                .with(
                    mockall::predicate::eq("instance-a"),
                    mockall::predicate::function(|a: &Patch<serde_json::Value>| match a {
                        Patch::Apply(v) => {
                            let su: Object<PartialInstanceSlotUsage, NotUsed> =
                                serde_json::from_value(v.clone()).unwrap();
                            error!("{:?}", su.spec.device_usage);
                            su.spec.device_usage
                                == HashMap::from([
                                    (
                                        "instance-a-0".to_string(),
                                        "C:akri.sh/config-a-1:node-a".to_owned(),
                                    ),
                                    ("instance-a-1".to_owned(), "node-a".to_owned()),
                                    ("instance-a-3".to_owned(), "node-a".to_owned()),
                                ])
                        }
                        _ => false,
                    }),
                    mockall::predicate::always(),
                )
                .returning(|_, _, _| {
                    Ok(Instance {
                        metadata: Default::default(),
                        spec: InstanceSpec {
                            configuration_name: "config-a".to_owned(),
                            cdi_name: Default::default(),
                            capacity: 1,
                            broker_properties: Default::default(),
                            shared: false,
                            nodes: Default::default(),
                            device_usage: Default::default(),
                        },
                    })
                });
            Box::new(api)
        });
        let kube_client = Arc::new(kube_client);
        let stopper = Stopper::new();
        let (s, _) = watch::channel(vec![
            DeviceUsage::Configuration {
                vdev: "akri.sh/config-a-1".to_owned(),
                node: "node-a".to_owned(),
            },
            DeviceUsage::Node("node-a".to_owned()),
            DeviceUsage::Node("node-b".to_owned()),
            DeviceUsage::Unused,
        ]);
        let instance_plugin = Arc::new(InstanceDevicePlugin {
            device: Device {
                name: "my-device".to_owned(),
                annotations: Default::default(),
                container_edits: ContainerEdit {
                    ..Default::default()
                },
            },
            slots_status: Mutex::new(s),
            node_name: "node-a".to_owned(),
            instance_name: "instance-a".to_owned(),
            instance_namespace: "namespace-a".to_owned(),
            kube_client,
            stopper: stopper.clone(),
        });

        assert!(instance_plugin
            .allocate(Request::new(AllocateRequest {
                container_requests: vec![ContainerAllocateRequest {
                    devices_i_ds: vec!["instance-a-0".to_owned()],
                }]
            }))
            .await
            .is_err());
        assert!(instance_plugin
            .allocate(Request::new(AllocateRequest {
                container_requests: vec![ContainerAllocateRequest {
                    devices_i_ds: vec!["instance-a-3".to_owned()],
                }]
            }))
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn test_list_and_watch() {
        let kube_client = Arc::new(MockIntoApi::new());
        let instance_plugin = Arc::new(
            InstanceDevicePlugin::new(
                "node-a".to_owned(),
                "instance-a".to_owned(),
                "namespace-a".to_owned(),
                Device {
                    name: "my-device".to_string(),
                    annotations: Default::default(),
                    container_edits: Default::default(),
                },
                &HashMap::from([("instance-a-1".to_owned(), "node-b".to_owned())]),
                3,
                kube_client,
            )
            .unwrap(),
        );
        let config_plugin =
            ConfigurationDevicePlugin::new("config-a".to_owned(), "node-a".to_owned());
        config_plugin
            .add_plugin("instance-a".to_owned(), instance_plugin.clone())
            .await;

        let mut instance_stream = instance_plugin.list_and_watch().await.unwrap().into_inner();
        let mut config_stream = config_plugin.list_and_watch().await.unwrap().into_inner();

        assert_eq!(
            instance_stream.next().await.unwrap().unwrap(),
            ListAndWatchResponse {
                devices: vec![
                    crate::plugin_manager::v1beta1::Device {
                        id: "instance-a-0".to_owned(),
                        health: "Healthy".to_owned(),
                        topology: None,
                    },
                    crate::plugin_manager::v1beta1::Device {
                        id: "instance-a-1".to_owned(),
                        health: "Unhealthy".to_owned(),
                        topology: None,
                    },
                    crate::plugin_manager::v1beta1::Device {
                        id: "instance-a-2".to_owned(),
                        health: "Healthy".to_owned(),
                        topology: None,
                    },
                ]
            }
        );
        // First message is sent before adding plugin
        assert_eq!(
            config_stream.next().await.unwrap().unwrap(),
            ListAndWatchResponse { devices: vec![] }
        );
        assert_eq!(
            config_stream.next().await.unwrap().unwrap(),
            ListAndWatchResponse {
                devices: vec![crate::plugin_manager::v1beta1::Device {
                    id: "config-a-0".to_owned(),
                    health: "Healthy".to_owned(),
                    topology: None,
                }]
            }
        );

        instance_plugin
            .update_slots(&HashMap::from([
                ("instance-a-0".to_owned(), "C:config-a-0:node-a".to_owned()),
                ("instance-a-1".to_owned(), "node-b".to_owned()),
                ("instance-a-2".to_owned(), "node-a".to_owned()),
            ]))
            .await
            .unwrap();

        assert_eq!(
            instance_stream.next().await.unwrap().unwrap(),
            ListAndWatchResponse {
                devices: vec![
                    crate::plugin_manager::v1beta1::Device {
                        id: "instance-a-0".to_owned(),
                        health: "Unhealthy".to_owned(),
                        topology: None,
                    },
                    crate::plugin_manager::v1beta1::Device {
                        id: "instance-a-1".to_owned(),
                        health: "Unhealthy".to_owned(),
                        topology: None,
                    },
                    crate::plugin_manager::v1beta1::Device {
                        id: "instance-a-2".to_owned(),
                        health: "Healthy".to_owned(),
                        topology: None,
                    },
                ]
            }
        );

        assert_eq!(
            config_stream.next().await.unwrap().unwrap(),
            ListAndWatchResponse {
                devices: vec![crate::plugin_manager::v1beta1::Device {
                    id: "config-a-0".to_owned(),
                    health: "Healthy".to_owned(),
                    topology: None,
                }]
            }
        );
    }
}
