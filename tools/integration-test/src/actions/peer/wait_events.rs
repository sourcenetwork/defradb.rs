use std::time::Duration;

use rapidhash::{HashSetExt, RapidHashSet};

use crate::actions::peer::support::addresses_of;
use crate::actions::shared::{EventType, Topic};
use crate::nodes::peer_id;
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// Waits for peers to join or leave topics, or starts listening for the peer events.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitForPeersEvents {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_type: Option<EventType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub millis: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topics: Option<Vec<Topic>>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub start: bool,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for WaitForPeersEvents {
    fn common(&self) -> &Common {
        &self.common
    }

    fn expected_data(&self) -> &Value {
        &self.expected_data
    }

    fn expected_error(&self) -> Option<&str> {
        self.expected_error.as_deref()
    }
    fn run(&self, state: &State) -> Result<Value, Failure> {
        self.common.plain(Self::name())?;
        let node = state.node(self.common.node())?;
        if self.start {
            node.watch().map_err(Failure::unsupported)?;
            return Ok(json!({}));
        }
        let mut wanted = RapidHashSet::new();
        for topic in self.topics.iter().flatten() {
            for peer in &topic.peers {
                let addresses = addresses_of(state, &[peer.as_str()])?;
                let id = addresses.first().map(|a| peer_id(a)).unwrap_or_default();
                wanted.insert((topic.topic.as_str().to_owned(), id.to_owned()));
            }
        }
        let kind = match self.event_type.unwrap_or_default() {
            EventType::Joined => "JOINED",
            EventType::Left => "LEFT",
        };
        let deadline = Duration::from_millis(self.millis.unwrap_or(5000));
        let events = node.events().map_err(Failure::operation)?;
        state
            .block_on(events.topics(wanted, kind, deadline))
            .map_err(Failure::operation)?;
        Ok(json!({}))
    }
}

impl Action for WaitForPeersEvents {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}
