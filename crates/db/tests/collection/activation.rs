use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use db::DB;
use schema::{CollectionVersion, FieldDescription, FieldKind};
use storage::keys::systemstore::CollectionKey;

use crate::common::failing_write_store::FailingWriteStore;

/// Activating a version deactivates the others in the same transaction; a
/// deactivation the store refuses fails the switch instead of leaving two
/// versions active.
#[test]
fn a_refused_deactivation_fails_the_version_switch() {
    workers::block_on(async {
        let armed = Arc::new(AtomicBool::new(false));
        let refusing = Arc::clone(&armed);
        let store =
            FailingWriteStore::in_memory(Arc::new(move |key: &[u8], value: Option<&[u8]>| {
                let deactivates = key
                    .strip_prefix(b"s")
                    .is_some_and(|key| key.starts_with(&CollectionKey::collection_prefix()))
                    && value
                        .and_then(|value| serde_json::from_slice::<CollectionVersion>(value).ok())
                        .is_some_and(|version| !version.is_active);
                if refusing.load(Ordering::SeqCst) && deactivates {
                    return Err(storage::Error::Other("deactivation refused".into()));
                }
                Ok(())
            }));
        let db = Arc::new(DB::new(store).unwrap());
        db.create_collection(CollectionVersion::new(
            "Note",
            "v1",
            "notes",
            vec![
                FieldDescription::new(
                    "1",
                    "_docID",
                    FieldKind::doc_id(),
                    schema::CType::LwwRegister,
                ),
                FieldDescription::new(
                    "2",
                    "title",
                    FieldKind::string(),
                    schema::CType::LwwRegister,
                ),
            ],
        ))
        .await
        .unwrap();
        let v1 = db
            .get_collection("Note")
            .unwrap()
            .unwrap()
            .version_id()
            .to_string();
        let v2 = db
        .patch_collection(
            "Note",
            r#"[{"op":"add","path":"/Note/Fields/-","value":{"Name":"extra","Kind":"String"}}]"#,
            None,
        )
        .await
        .unwrap()
        .version_id;

        armed.store(true, Ordering::SeqCst);
        let switched = db.set_active_collection_version(&v1).await;

        assert!(
            switched.is_err(),
            "the switch must fail when the old version cannot be deactivated, got {switched:?}"
        );
        assert_eq!(
            db.get_collection("Note").unwrap().unwrap().version_id(),
            v2,
            "nothing of the failed switch commits"
        );
    })
    .unwrap()
}
