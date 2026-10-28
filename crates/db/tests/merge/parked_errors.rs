//! How a parked block's replay classes the error it hit: the block's own,
//! the store's, or a key this node does not hold.

use db::merge::MergeError;

#[test]
fn a_parked_block_fails_only_for_an_error_of_its_own() {
    let store = || storage::Error::EmptyKey;
    let own = [
        MergeError::MergeFailed("bad operand".into()),
        MergeError::BlockDecode("truncated".into()),
        MergeError::Database(db::Error::CollectionNotFound("Score".into())),
    ];
    for error in own {
        assert!(error.fails_block(), "{error}");
        assert!(!error.needs_key(), "{error}");
    }
    let stores = [
        MergeError::Storage("disk full".into()),
        MergeError::Database(db::Error::Storage(store())),
        MergeError::Database(db::Error::Datastore(datastore::Error::Storage(store()))),
    ];
    for error in stores {
        assert!(!error.fails_block(), "{error}");
        assert!(!error.needs_key(), "{error}");
    }
    let keys = [
        MergeError::Kms(kms::Error::KeyUnavailable),
        MergeError::Kms(kms::Error::AccessDenied {
            reason: "not granted".into(),
        }),
    ];
    for error in keys {
        assert!(!error.fails_block(), "{error}");
        assert!(error.needs_key(), "{error}");
    }
}
