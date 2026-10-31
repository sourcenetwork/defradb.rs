mod add;
mod delete;
mod delete_with_filter;
mod purge;
mod sync;
mod update;
mod update_with_filter;

pub use add::AddDoc;
pub use delete::DeleteDoc;
pub use delete_with_filter::DeleteWithFilter;
pub use purge::PurgeDocs;
pub use sync::SyncDocs;
pub use update::UpdateDoc;
pub use update_with_filter::UpdateWithFilter;
