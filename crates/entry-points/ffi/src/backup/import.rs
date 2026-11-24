use std::ffi::c_char;
use std::fs;

use acp::nac::NodePermission;
use db::backup::{DbJob, ImportHost};
use defra_core::thread_bounds::MaybeBoxFuture;
use query::{QueryRequest, QueryResponse};

use crate::document::run::DocumentCall;
use crate::ffi_entry;
use crate::request::Request;
use crate::state::NODES;
use crate::target::go_string;
use crate::types::{CollectionOptions, DefraResult};
use crate::{ffi_async_ok, try_ffi, ERR_INVALID_NODE_HANDLE};

/// Import documents from a JSON backup file.
///
/// The file must be a JSON object mapping collection names to arrays of documents:
/// ```json
/// {
///     "User": [{"_docID": "...", "_docIDNew": "...", "name": "John", "age": 30}],
///     "Address": [{"_docID": "...", "_docIDNew": "...", "street": "...", "city": "..."}]
/// }
/// ```
///
/// Self-referencing FK fields are stripped before creation and applied
/// via update afterward, matching Go DefraDB behavior. Every create and
/// patch is a document call of the caller, signed and broadcast as
/// `defra_document_add`'s writes are.
///
/// # Safety
///
/// `filepath` must be null or a valid null-terminated string.
pub unsafe fn defra_backup_import(
    node_ptr: usize,
    filepath: *const c_char,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::resolve(node_ptr, identity_ptr));
        let node_ptr = try_ffi!(request.require_node("backup import"));
        let path_str = go_string(filepath);

        let (database, pool) = match NODES.get(node_ptr, |state| {
            (state.database.clone(), state.database.pool().cloned())
        }) {
            Some(r) => r,
            None => return DefraResult::error(ERR_INVALID_NODE_HANDLE),
        };
        let call = try_ffi!(DocumentCall::begin(
            node_ptr,
            &CollectionOptions::default(),
            identity_ptr,
            NodePermission::DocumentUpdate,
        ));
        let context = call.context();
        let target = call.target().clone();
        let host = FfiHost { call, pool };
        ffi_async_ok!(target &target, {
            let content = fs::read_to_string(&path_str).map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    format!("failed to open file '{}': {}", path_str, e)
                } else {
                    format!("failed to read file '{}': {}", path_str, e)
                }
            })?;

            db::backup::import_database_as(&context, &database, &host, &content)
                .await
                .map(|_| ())
        })
    }
}

/// The FFI's request path for an import's writes: each is a document call
/// of the caller; store jobs go to the node's workers.
struct FfiHost {
    call: DocumentCall,
    pool: Option<workers::Pool>,
}

impl ImportHost for FfiHost {
    fn run(&self, request: QueryRequest) -> MaybeBoxFuture<'_, QueryResponse> {
        Box::pin(self.call.execute(request))
    }

    fn on_db(&self, job: DbJob) -> MaybeBoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            workers::run_on(self.pool.as_ref(), workers::Target::LeastLoaded, job)
                .await
                .unwrap_or_else(|error| Err(error.to_string()))
        })
    }
}
