use document::Document;
use schema::{
    CType, CollectionBuilder, CollectionVersion, FieldDescription, FieldKind, ScalarKind,
};

pub fn collection(fields: &[&str]) -> CollectionVersion {
    fields
        .iter()
        .enumerate()
        .fold(
            CollectionBuilder::new("Test", "bafytest"),
            |builder, (i, name)| {
                builder.field(FieldDescription::new(
                    i.to_string(),
                    *name,
                    FieldKind::Scalar(ScalarKind::None),
                    CType::LwwRegister,
                ))
            },
        )
        .build()
}

pub fn doc(fields: &[&str]) -> Document {
    Document::new(collection(fields))
}

pub fn from_json(json: &str) -> document::Result<Document> {
    let value: serde_json::Value = serde_json::from_str(json)?;
    let names: Vec<&str> = value
        .as_object()
        .map(|o| o.keys().map(String::as_str).collect())
        .unwrap_or_default();
    Document::from_json_str(json, collection(&names))
}

pub fn from_cbor(bytes: &[u8]) -> document::Result<Document> {
    let names: Vec<String> = match ciborium::from_reader::<ciborium::Value, _>(bytes) {
        Ok(ciborium::Value::Map(entries)) => entries
            .into_iter()
            .filter_map(|(k, _)| k.into_text().ok())
            .collect(),
        _ => Vec::new(),
    };
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    Document::from_cbor(bytes, collection(&names))
}
