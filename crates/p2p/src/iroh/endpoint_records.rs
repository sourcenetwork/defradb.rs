//! The endpoint side of service records: how the records task announces,
//! sends, looks up pointers and hints gossip joins.

use bytes::Bytes;
use iroh::address_lookup::UserData;
use iroh::EndpointId;
use n0_future::time::Duration;
use n0_future::StreamExt;
use tracing::debug;

use super::endpoint::{peer_direct_addr, spawn_task, EndpointResources};
use super::endpoint_rpc::handle_fire_and_forget;
use super::peer_map::endpoint_id_to_peer_id;
use super::protocols::STREAM_SERVICE_RECORDS;
use super::records::{Input, Net, Pointer, RecordsHandle, RecordsMessage, RECORDS_TOPIC};

/// How long a pointer lookup may take before the peer counts as having none.
const RESOLVE_DEADLINE: Duration = Duration::from_secs(5);

pub(super) struct EndpointNet {
    resources: EndpointResources,
    records: RecordsHandle,
}

impl EndpointNet {
    pub(super) fn new(resources: EndpointResources, records: RecordsHandle) -> Self {
        Self { resources, records }
    }
}

impl Net for EndpointNet {
    fn announce(&self, record: Bytes) {
        let gossip = self.resources.gossip.clone();
        let _ = spawn_task(&self.resources.spawned_tasks, async move {
            gossip
                .publish(RECORDS_TOPIC.to_owned(), record, false)
                .await;
        });
    }

    fn set_pointer(&self, pointer: Pointer) {
        match pointer.print().parse::<UserData>() {
            Ok(user_data) => self
                .resources
                .endpoint
                .set_user_data_for_address_lookup(Some(user_data)),
            Err(error) => debug!(%error, "service record pointer does not fit the user data"),
        }
    }

    fn send(&self, peer: EndpointId, message: RecordsMessage) {
        let resources = self.resources.clone();
        let _ = spawn_task(&self.resources.spawned_tasks, async move {
            let peer_id = endpoint_id_to_peer_id(&peer);
            let direct_addr = peer_direct_addr(&resources.peer_map, &peer_id);
            if let Err(error) = handle_fire_and_forget(
                &resources.endpoint,
                &peer_id,
                STREAM_SERVICE_RECORDS,
                &message,
                direct_addr,
                &resources.connection_cache,
                &resources.admission,
            )
            .await
            {
                debug!(%peer_id, %error, "service records message not sent");
            }
        });
    }

    fn resolve(&self, peer: EndpointId) {
        let endpoint = self.resources.endpoint.clone();
        let records = self.records.clone();
        let _ = spawn_task(&self.resources.spawned_tasks, async move {
            let pointer =
                n0_future::time::timeout(RESOLVE_DEADLINE, lookup_pointer(&endpoint, peer))
                    .await
                    .ok()
                    .flatten();
            records.push(Input::Pointer {
                node: peer,
                pointer,
            });
        });
    }

    fn join_hint(&self, topic: String, peers: Vec<EndpointId>) {
        self.resources.gossip.hint(topic, peers);
    }
}

/// The first pointer any address lookup returns for `peer`.
async fn lookup_pointer(endpoint: &iroh::Endpoint, peer: EndpointId) -> Option<Pointer> {
    let mut items = endpoint.address_lookup().ok()?.resolve(peer);
    while let Some(item) = items.next().await {
        let Ok(Ok(item)) = item else {
            continue;
        };
        if let Some(pointer) = item
            .user_data()
            .and_then(|data| Pointer::parse(data.as_ref()))
        {
            return Some(pointer);
        }
    }
    None
}
