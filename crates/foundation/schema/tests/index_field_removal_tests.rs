use schema::definition_validation::validate_collection_changes;
use schema::{
    CType, CollectionVersion, EncryptedIndexDescription, FieldDescription, FieldKind,
    IndexDescription,
};

fn users(fields: &[&str]) -> CollectionVersion {
    let mut all = vec![FieldDescription::new(
        "1",
        "_docID",
        FieldKind::doc_id(),
        CType::LwwRegister,
    )];
    for (id, name) in (2..).zip(fields) {
        all.push(FieldDescription::new(
            id.to_string(),
            *name,
            FieldKind::string(),
            CType::LwwRegister,
        ));
    }
    let mut users = CollectionVersion::new("Users", "v1", "users", all);
    users.is_active = true;
    users
}

fn with_name_index(mut users: CollectionVersion) -> CollectionVersion {
    users.indexes = vec![IndexDescription::new("Users_name_ASC").with_field("name", false)];
    users
}

#[test]
fn removing_an_indexed_field_names_the_index() {
    let old = with_name_index(users(&["age", "name"]));
    let new = with_name_index(users(&["age"]));
    let error = validate_collection_changes(&[old], &[new]).unwrap_err();
    assert_eq!(
        error,
        "cannot remove field 'name': index 'Users_name_ASC' depends on it, drop the index first. Collection: Users"
    );
}

#[test]
fn removing_a_field_no_index_reads_is_accepted() {
    let old = with_name_index(users(&["name", "age"]));
    let new = with_name_index(users(&["name"]));
    validate_collection_changes(&[old], &[new]).unwrap();
}

#[test]
fn removing_a_field_with_an_encrypted_index_is_refused() {
    let mut old = users(&["age", "name"]);
    old.encrypted_indexes = vec![EncryptedIndexDescription::new("name")];
    let mut new = users(&["age"]);
    new.encrypted_indexes = old.encrypted_indexes.clone();
    let error = validate_collection_changes(&[old], &[new]).unwrap_err();
    assert!(
        error.starts_with("cannot remove field 'name': an encrypted index depends on it"),
        "{error}"
    );
}
