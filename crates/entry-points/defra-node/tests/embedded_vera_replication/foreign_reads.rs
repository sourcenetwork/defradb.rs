//! Reads on the guest's node of a document under the owner's policy: user
//! queries and `_commits` queries, two separate read paths, are both
//! decided by the owner's home. An actor that home does not grant `read`
//! sees nothing of the document on either path, a granted one sees it on
//! both, and with the home away past the freshness bound each path reports
//! the failed check as it does for any provider that cannot answer
//! (`reqs/defra/acp.md` `defra.acp.foreign_policy`).

use defra_core::RequestContext;
use defra_node::{DocumentPermission, QueryRequest, QueryResponse};
use identity::Did;

use super::actor::Actor;
use super::foreign::Shared;
use super::peer::Peer;
use super::wait::eventually;

/// The user query of every `Users` row.
const ROWS: &str = "{ Users { _docID } }";

/// The `_commits` query of `doc`'s composite commits.
fn commits_of(doc: &str) -> String {
    format!("{{ _commits(docID: \"{doc}\", filter: {{fieldName: {{_eq: \"_C\"}}}}) {{ cid }} }}")
}

impl Peer {
    /// Whether `reader` sees `doc` on neither read path, while this node's
    /// provider answers that it may not read it: a denial, not a check that
    /// went unanswered.
    async fn denies_reading(&self, reader: &Did, doc: &str) -> bool {
        matches!(
            self.may(reader, DocumentPermission::Read, doc).await,
            Ok(false)
        ) && self.row(reader, doc).await.is_none()
            && self.commits(reader, doc).await.is_empty()
    }

    /// `query` as `reader`, failing every access check that has no answer
    /// instead of leaving its row out.
    async fn strictly(&self, reader: &Did, query: &str) -> QueryResponse {
        let context = RequestContext::default()
            .with_identity(Some(reader.to_string()))
            .with_strict_access_reads();
        self.node
            .execute_request(QueryRequest::new(query).with_context(context))
            .await
    }

    /// With the owner's home away past the freshness bound, how both read
    /// paths report `reader`'s reads of `doc`: a user query leaves the row
    /// out without an error, a `_commits` query fails, and a strict read of
    /// either fails with the retryable "access check unavailable". None of
    /// them is answered as a denial, and none returns the document.
    pub async fn reads_with_the_home_away(&self, reader: &Did, doc: &str) {
        let default = self
            .node
            .execute_request(QueryRequest::new(ROWS).with_identity(Some(reader.clone())))
            .await;
        assert!(!default.has_errors(), "{:?}", default.errors);
        let rows = &default.data.expect("data")["Users"];
        assert!(
            rows.as_array().expect("rows").is_empty(),
            "a row whose check has no answer is left out: {rows}"
        );

        let history = self
            .node
            .execute_request(QueryRequest::new(commits_of(doc)).with_identity(Some(reader.clone())))
            .await;
        assert!(history.data.is_none(), "{:?}", history.data);
        assert!(
            history
                .errors
                .iter()
                .any(|error| error.message == "unable to verify access to commit history"),
            "{:?}",
            history.errors
        );

        for query in [ROWS.to_owned(), commits_of(doc)] {
            let strict = self.strictly(reader, &query).await;
            assert!(strict.data.is_none(), "{query}: {:?}", strict.data);
            assert_eq!(strict.errors.len(), 1, "{query}: {:?}", strict.errors);
            assert_eq!(
                strict.errors[0].code(),
                Some(query::executor::ACCESS_CHECK_UNAVAILABLE_ERROR_CODE),
                "{query}: {:?}",
                strict.errors
            );
        }
    }
}

/// On the guest's node an actor the owner's home does not grant `read`
/// sees no row of the owner's document in a user query and no commit of it
/// in a `_commits` query, while the granted guest sees both; once the home
/// grants the actor, it sees both, and once the home revokes the guest, the
/// guest sees neither.
#[test]
#[mantra_macros::req_test("defra.acp.foreign_policy")]
fn on_the_guests_node_only_whom_the_owners_home_grants_reads_the_owners_document() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let shared = Shared::new(directory.path(), (9469, 9470), None).await;
        let (a, b, doc) = (&shared.a, &shared.b, &shared.doc);
        let (owner, guest) = (&shared.owner, &shared.guest);
        let stranger = Actor::new(66);

        assert!(b.answer(&guest.did, DocumentPermission::Read, doc).await);
        assert_eq!(b.visible_points(&guest.did, doc).await, 0);
        let history = a.commits(&owner.did, doc).await;
        assert!(!history.is_empty());
        assert_eq!(b.commits(&guest.did, doc).await, history);
        assert!(
            b.denies_reading(&stranger.did, doc).await,
            "an actor the owner's home does not grant reads nothing"
        );

        a.grant(doc, "reader", &stranger.did).await;
        eventually(
            "the owner's grant lets the actor read on both paths",
            || async {
                b.row(&stranger.did, doc).await.is_some()
                    && b.commits(&stranger.did, doc).await == history
            },
        )
        .await;

        a.revoke(doc, "reader", &guest.did).await;
        a.revoke(doc, "writer", &guest.did).await;
        eventually(
            "the owner's revocation hides the document from the guest",
            || async { b.denies_reading(&guest.did, doc).await },
        )
        .await;
        shared.stop().await;
    })
    .unwrap();
}
