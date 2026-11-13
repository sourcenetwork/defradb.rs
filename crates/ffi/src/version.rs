use std::ffi::c_int;

use defra_version::VersionInfo;

use crate::ffi_entry;
use crate::types::DefraResult;

/// Returns DefraDB version information: as JSON when `flag_json` is set,
/// otherwise as text, with full build information when `flag_full` is set.
/// The same output as `defradb version`.
#[no_mangle]
pub extern "C" fn defra_version_get(flag_full: c_int, flag_json: c_int) -> DefraResult {
    ffi_entry! {
        let info = VersionInfo::new();
        if flag_json != 0 {
            return DefraResult::from(
                serde_json::to_string(&info).map_err(|error| error.to_string()),
            );
        }
        if flag_full != 0 {
            DefraResult::success(info.full())
        } else {
            DefraResult::success(info.short())
        }
    }
}
