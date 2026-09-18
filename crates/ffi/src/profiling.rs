use std::ffi::c_char;
use std::fs::File;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use kovan_queue::seg_queue::SegQueue;
use tracing_chrome::{ChromeLayerBuilder, FlushGuard};
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

use crate::helpers::require_c_str;
use crate::types::FfiResult;
use crate::{ffi_entry, try_ffi};

static PROFILING_GUARD: OnceLock<SegQueue<FlushGuard>> = OnceLock::new();
static PROFILING_RUNNING: AtomicBool = AtomicBool::new(false);

fn guard_slot() -> &'static SegQueue<FlushGuard> {
    PROFILING_GUARD.get_or_init(SegQueue::new)
}

fn with_default_transport_noise_filters(filter: EnvFilter) -> EnvFilter {
    filter
        .add_directive(
            "iroh_quinn_proto::connection=error"
                .parse()
                .expect("valid tracing directive"),
        )
        .add_directive(
            "noq_proto::connection=error"
                .parse()
                .expect("valid tracing directive"),
        )
}

fn install_chrome_layer(output_path: &str) -> Result<FlushGuard, String> {
    let file = File::create(output_path).map_err(|error| {
        format!(
            "failed to create profiling trace file {}: {}",
            output_path, error
        )
    })?;

    let filter = with_default_transport_noise_filters(
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
    );
    let (chrome_layer, flush_guard) = ChromeLayerBuilder::new()
        .writer(file)
        .include_args(true)
        .build();

    let subscriber = tracing_subscriber::registry()
        .with(filter)
        .with(chrome_layer);

    tracing::subscriber::set_global_default(subscriber)
        .map_err(|error| format!("failed to initialize profiling subscriber: {}", error))?;

    Ok(flush_guard)
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn defra_profiling_start(output_path: *const c_char) -> FfiResult {
    ffi_entry! {
        let output_path = try_ffi!(unsafe { require_c_str(output_path, "output_path") });

        if PROFILING_RUNNING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return FfiResult::error("profiling is already running");
        }

        match install_chrome_layer(&output_path) {
            Ok(flush_guard) => {
                guard_slot().push(flush_guard);
                FfiResult::ok()
            }
            Err(error) => {
                PROFILING_RUNNING.store(false, Ordering::Release);
                FfiResult::error(error)
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn defra_profiling_stop() -> FfiResult {
    ffi_entry! {
        let Some(flush_guard) = guard_slot().pop() else {
            return FfiResult::error("profiling is not running");
        };

        drop(flush_guard);
        PROFILING_RUNNING.store(false, Ordering::Release);
        FfiResult::ok()
    }
}
