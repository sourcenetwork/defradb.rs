use pgwire::api::Type;
use schema::{FieldKind, ScalarKind};

/// Map a DefraDB FieldKind to a PostgreSQL wire protocol type.
pub fn field_kind_to_pg_type(kind: &FieldKind) -> Type {
    match kind {
        FieldKind::Scalar(scalar) => scalar_to_pg_type(*scalar),
        FieldKind::ScalarArray(_) => Type::JSONB,
        FieldKind::Relation { .. } | FieldKind::SelfRef { .. } | FieldKind::Named { .. } => {
            Type::JSONB
        }
        _ => unreachable!(),
    }
}

fn scalar_to_pg_type(kind: ScalarKind) -> Type {
    match kind.base_kind() {
        ScalarKind::None => Type::TEXT,
        ScalarKind::DocID => Type::TEXT,
        ScalarKind::NillableBool => Type::BOOL,
        ScalarKind::NillableInt => Type::INT8,
        ScalarKind::NillableFloat64 => Type::FLOAT8,
        ScalarKind::NillableFloat32 => Type::FLOAT4,
        ScalarKind::NillableDateTime => Type::TIMESTAMPTZ,
        ScalarKind::NillableString => Type::TEXT,
        ScalarKind::NillableBlob => Type::BYTEA,
        ScalarKind::NillableJson => Type::JSONB,
        _ => Type::TEXT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_kinds_map_correctly() {
        assert_eq!(scalar_to_pg_type(ScalarKind::NillableInt), Type::INT8);
        assert_eq!(scalar_to_pg_type(ScalarKind::NillableString), Type::TEXT);
        assert_eq!(scalar_to_pg_type(ScalarKind::NillableBool), Type::BOOL);
        assert_eq!(scalar_to_pg_type(ScalarKind::NillableFloat64), Type::FLOAT8);
        assert_eq!(scalar_to_pg_type(ScalarKind::DocID), Type::TEXT);
    }

    #[test]
    fn relation_kinds_map_to_jsonb() {
        let kind = FieldKind::relation("col1", false);
        assert_eq!(field_kind_to_pg_type(&kind), Type::JSONB);
    }
}
