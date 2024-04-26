use std::{
    collections::{HashMap, HashSet},
    convert::TryFrom,
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::net::UnixStream;
use tonic::transport::{Endpoint, Uri};
use tower::service_fn;

use crate::plugin_manager::v1::ListPodResourcesRequest;

use super::{
    device_plugin_instance_controller::DevicePluginManager,
    v1::pod_resources_lister_client as podresources,
};

/// Path of the Kubelet registry socket
pub const KUBELET_SOCKET: &str = "/var/lib/kubelet/pod-resources/kubelet.sock";
const SLOT_GRACE_PERIOD: Duration = Duration::from_secs(20);
const SLOT_RECLAIM_INTERVAL: Duration = Duration::from_secs(10);

async fn get_used_slots() -> Result<HashSet<String>, anyhow::Error> {
    // We will ignore this dummy uri because UDS does not use it.
    // Some servers will check the uri content so the uri needs to
    // be in valid format even it's not used, the scheme part is used
    // to specific what scheme to use, such as http or https
    let kubelet_socket_closure = KUBELET_SOCKET.to_string();
    let channel = Endpoint::try_from("http://[::1]:50051")
        .unwrap()
        .connect_with_connector(service_fn(move |_: Uri| {
            UnixStream::connect(kubelet_socket_closure.clone())
        }))
        .await?;
    let mut podresources_client = podresources::PodResourcesListerClient::new(channel);

    let list_request = tonic::Request::new(ListPodResourcesRequest {});
    trace!(
        "register - before call to register with the kubelet at socket {}",
        KUBELET_SOCKET
    );

    // Get the list of allocated device ids from kubelet
    let resources = podresources_client
        .list(list_request)
        .await?
        .into_inner()
        .pod_resources
        .into_iter()
        .flat_map(|pr| {
            pr.containers.into_iter().flat_map(|cr| {
                cr.devices.into_iter().flat_map(|cd| {
                    if cd.resource_name.starts_with("akri.sh/") {
                        cd.device_ids
                    } else {
                        vec![]
                    }
                })
            })
        })
        .collect();

    Ok(resources)
}

pub async fn start_reclaimer(dp_manager: Arc<DevicePluginManager>) {
    let mut stalled_slots: HashMap<String, Instant> = HashMap::new();
    let mut signal =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
    loop {
        trace!("reclaiming unused slots - start");
        if let Ok(used_slots) = get_used_slots().await {
            trace!("used slots: {:?}", used_slots);
            let theoretical_slots = dp_manager.get_used_slots().await;
            trace!("theoretical slots: {:?}", theoretical_slots);
            let mut new_stalled_slots: HashMap<String, Instant> = HashMap::new();
            let reclaim_iteration_start = Instant::now();
            for slot_to_reclaim in theoretical_slots.difference(&used_slots) {
                // See if slot was already stalled at previous iteration
                if let Some(at) = stalled_slots.get(slot_to_reclaim) {
                    if reclaim_iteration_start.saturating_duration_since(*at) >= SLOT_GRACE_PERIOD {
                        // Slot is stalled for more than grace period, free it
                        trace!("freeing slot: {}", slot_to_reclaim);
                        if dp_manager
                            .free_slot(slot_to_reclaim.to_string())
                            .await
                            .is_err()
                        {
                            new_stalled_slots.insert(slot_to_reclaim.to_string(), at.to_owned());
                        };
                    } else {
                        // Keep slot as stall
                        new_stalled_slots.insert(slot_to_reclaim.to_string(), at.to_owned());
                    }
                } else {
                    // Mark slot as stall
                    new_stalled_slots.insert(slot_to_reclaim.to_string(), reclaim_iteration_start);
                }
            }
            stalled_slots = new_stalled_slots;
        }
        tokio::select! {
            _ = tokio::time::sleep(SLOT_RECLAIM_INTERVAL) => {},
            _ = signal.recv() => return,
        };
    }
}
