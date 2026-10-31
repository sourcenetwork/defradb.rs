//! Preserve runtime schema fields in exact validator inputs without changing
//! the persisted CollectionVersion representation: the root ID, and each
//! index's unset kind and auto-generated marker, which the persisted form
//! normalizes and drops.
use schema::CollectionVersion;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// What the persisted form of one index loses.
#[derive(Serialize, Deserialize)]
struct IndexRuntime {
    kindless: bool,
    auto_generated: bool,
}

pub(super) fn serialize<S>(collection: &CollectionVersion, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    #[derive(Serialize)]
    struct Input<'a> {
        definition: &'a CollectionVersion,
        root_id: u32,
        indexes: Vec<IndexRuntime>,
    }
    Input {
        definition: collection,
        root_id: collection.root_id,
        indexes: collection
            .indexes
            .iter()
            .map(|index| IndexRuntime {
                kindless: index.kind.is_none(),
                auto_generated: index.auto_generated,
            })
            .collect(),
    }
    .serialize(serializer)
}

pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<CollectionVersion, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    struct Input {
        definition: CollectionVersion,
        root_id: u32,
        indexes: Vec<IndexRuntime>,
    }
    let mut input = Input::deserialize(deserializer)?;
    input.definition.root_id = input.root_id;
    for (index, runtime) in input.definition.indexes.iter_mut().zip(input.indexes) {
        if runtime.kindless {
            index.kind = None;
        }
        index.auto_generated = runtime.auto_generated;
    }
    Ok(input.definition)
}

#[cfg(test)]
mod tests {
    use schema::{CollectionVersion, IndexDescription};
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    struct Carried(#[serde(with = "super")] CollectionVersion);

    #[test]
    fn round_trip_keeps_root_id_unset_kinds_and_auto_generated_indexes() {
        let mut collection = CollectionVersion::new("User", "v1", "c1", Vec::new());
        collection.root_id = 7;
        let mut generated = IndexDescription::new("generated").with_field("name", false);
        generated.auto_generated = true;
        collection.indexes = vec![
            generated,
            IndexDescription::new("plain").with_field("age", true),
        ];

        let bytes = serde_json::to_vec(&Carried(collection.clone())).unwrap();
        let Carried(back) = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(back.root_id, 7);
        assert_eq!(back.indexes, collection.indexes);
    }
}
