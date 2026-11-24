//! Subscriptions, read by polling.
//!
//! `defra_query_execute` starts a GraphQL subscription and
//! `defra_event_subscribe` an event subscription; each answers with an ID
//! (status 2). The caller polls that ID for results and closes it when done.

use std::ffi::c_char;

mod events;

pub use events::defra_event_subscribe;

use crate::ffi_entry;
use crate::state::{GRAPHQL_SUBSCRIPTIONS, NODES};
use crate::target::go_string;
use crate::types::DefraResult;

pub(crate) use query::subscription::response_has_data;

/// What polling an ID no subscription has reports.
const ERR_INVALID_SUBSCRIPTION_ID: &str = "invalid subscription ID";

/// What polling a subscription that has ended reports.
const ERR_GETTING_SUBSCRIPTION: &str = "could not retrieve subscription";

/// Returns the next pending result of subscription `id`: status 0 with the
/// GraphQL result as JSON, or status 2 with no value when none is pending.
/// A subscription that has ended reports status 1 and is closed.
///
/// # Safety
///
/// `id` must be null or a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn defra_subscription_poll(id: *const c_char) -> DefraResult {
    ffi_entry! {
        let id = go_string(id);
        let Some(state) = id.parse().ok().and_then(|handle| GRAPHQL_SUBSCRIPTIONS.get(handle)) else {
            return DefraResult::error(format!("{}. SubscriptionID: {}", ERR_INVALID_SUBSCRIPTION_ID, id));
        };
        match state.result_receiver.try_recv() {
            Some(json) => DefraResult::success(json),
            None if state.result_receiver.is_disconnected() => {
                close(&id);
                DefraResult::error(ERR_GETTING_SUBSCRIPTION)
            }
            None => DefraResult {
                status: 2,
                ..DefraResult::ok()
            },
        }
    }
}

/// Cancels subscription `id`. Closing an ID no subscription has does
/// nothing.
///
/// # Safety
///
/// `id` must be null or a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn defra_subscription_close(id: *const c_char) -> DefraResult {
    ffi_entry! {
        close(&go_string(id));
        DefraResult::ok()
    }
}

/// Stops subscription `id`'s service and releases its event-bus
/// subscription.
fn close(id: &str) {
    let Some(state) = id
        .parse()
        .ok()
        .and_then(|handle| GRAPHQL_SUBSCRIPTIONS.remove(handle))
    else {
        return;
    };
    state.service.stop();
    NODES.get(state.node_handle, |node| {
        node.event_bus.unsubscribe(state.event_sub_id);
    });
}

#[cfg(test)]
mod tests;
