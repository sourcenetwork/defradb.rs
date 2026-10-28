mod delete;
mod delete_with_filter;
mod parallel;
mod save;
mod sync;
mod update;
mod update_with_filter;

pub use delete::DeleteDoc;
pub use delete_with_filter::DeleteWithFilter;
pub use parallel::ParallelDocs;
pub use save::SaveDoc;
pub use sync::SyncDocs;
pub use update::UpdateDoc;
pub use update_with_filter::UpdateWithFilter;
