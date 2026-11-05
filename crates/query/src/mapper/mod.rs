//! Query mapper types for converting parsed queries to internal operations

mod cursor;
mod filter;
mod input_op;
mod mutation;
mod mutation_selection;
mod types;

pub use cursor::{CursorAliases, CursorPageInfoFields, CursorParams};
pub use filter::{like_pattern_match, Filter, FilterOp};
pub(crate) use input_op::two_ops;
pub use input_op::{split_update_input, InputOp, FIELD_OPS};
pub use mutation::{parse_mutation_name, Mutation, MutationType};
pub use mutation_selection::valid_unaliased_doc_id_selection;
pub use mutation_selection::single_document_mutation_target;
pub use types::{
    Aggregate, AggregateTarget, AggregateType, Field, FullTextSearch, GroupBy, Limit, OrderBy,
    OrderCondition, OrderDirection, Requestable, Select, SelectionType, Similarity,
};
