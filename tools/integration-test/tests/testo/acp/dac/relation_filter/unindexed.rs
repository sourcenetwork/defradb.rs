//! Every shape, planned as full scans of the related collection.

use super::support::{shape_tests, Indexes};

shape_tests!(
    Indexes::None;
    many_to_one_eq,
    many_to_one_like,
    many_to_one_selected,
    many_to_one_or,
    many_to_one_not,
    one_to_one_secondary,
    one_to_many,
    one_to_many_selected,
    multi_level,
    count,
    nested_count,
    group_by,
    secondary_id,
    order,
);
