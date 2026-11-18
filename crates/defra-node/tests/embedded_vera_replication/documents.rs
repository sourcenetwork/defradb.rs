//! Reading and writing the `Users` collection on a peer, and asking its
//! Vera node about the documents in it.

use std::time::Instant;

use acp::DocumentACP as _;
use defra_node::{DocumentPermission, QueryRequest};
use identity::Did;

use super::actor::Actor;
use super::peer::Peer;
use super::wait::{pause, PATIENCE};

impl Peer {
    /// `actor`'s permission on `doc` as this peer's Vera node answers it.
    pub async fn may(&self, actor: &Did, permission: DocumentPermission, doc: &str) -> bool {
        self.node
            .vera_acp()
            .expect("vera.rs document ACP")
            .check_doc_access(
                &acp::Identity::Authenticated(actor.clone()),
                permission,
                &self.policy,
                "users",
                doc,
            )
            .await
            .unwrap_or(false)
    }

    /// Who this peer's Vera node says owns `doc`, if it answers.
    pub async fn owner_of(&self, doc: &str) -> Option<Did> {
        self.node
            .vera_acp()
            .expect("vera.rs document ACP")
            .get_doc_owner(&self.policy, "users", doc)
            .await
            .ok()
            .flatten()
    }

    /// Grant `relation` on `doc` to `target`, as this peer's owner.
    pub async fn grant(&self, doc: &str, relation: &str, target: &Did) {
        self.node
            .add_dac_actor_relationship(
                self.owner().did.as_str(),
                "Users",
                doc,
                relation,
                target.as_str(),
            )
            .await
            .expect("grant");
    }

    /// Revoke `relation` on `doc` from `target`, as this peer's owner.
    pub async fn revoke(&self, doc: &str, relation: &str, target: &Did) {
        self.node
            .delete_dac_actor_relationship(
                self.owner().did.as_str(),
                "Users",
                doc,
                relation,
                target.as_str(),
            )
            .await
            .expect("revoke");
    }

    /// The `Users` rows `reader` sees, by document ID.
    pub async fn rows(&self, reader: &Did) -> Vec<Row> {
        let response = self
            .node
            .execute_request(
                QueryRequest::new("{ Users { _docID name points } }")
                    .with_identity(Some(reader.clone())),
            )
            .await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        let mut rows: Vec<Row> = response.data.expect("data")["Users"]
            .as_array()
            .expect("rows")
            .iter()
            .map(|row| Row {
                doc: row["_docID"].as_str().expect("document ID").to_owned(),
                name: row["name"].as_str().map(str::to_owned),
                points: row["points"].as_i64(),
            })
            .collect();
        rows.sort_by(|left, right| left.doc.cmp(&right.doc));
        rows
    }

    /// `doc` as `reader` sees it.
    pub async fn row(&self, reader: &Did, doc: &str) -> Option<Row> {
        self.rows(reader)
            .await
            .into_iter()
            .find(|row| row.doc == doc)
    }

    /// The points of `doc` as `reader` sees them.
    pub async fn points(&self, reader: &Did, doc: &str) -> Option<i64> {
        self.row(reader, doc).await.and_then(|row| row.points)
    }

    /// The composite commits this peer holds merged for `doc`, sorted. The
    /// query decides the history's access through the node's provider, and
    /// is asked again while that decision fails, as on a loaded machine the
    /// provider can be too late to answer for a moment.
    pub async fn commits(&self, reader: &Did, doc: &str) -> Vec<String> {
        let request = QueryRequest::new(format!(
            "{{ _commits(docID: \"{doc}\", filter: {{fieldName: {{_eq: \"_C\"}}}}) {{ cid }} }}"
        ))
        .with_identity(Some(reader.clone()));
        let deadline = Instant::now() + PATIENCE;
        let response = loop {
            let response = self.node.execute_request(request.clone()).await;
            if !response.has_errors() || Instant::now() >= deadline {
                break response;
            }
            pause().await;
        };
        assert!(!response.has_errors(), "{:?}", response.errors);
        let mut cids: Vec<String> = response.data.expect("data")["_commits"]
            .as_array()
            .expect("commits")
            .iter()
            .map(|commit| commit["cid"].as_str().expect("cid").to_owned())
            .collect();
        cids.sort();
        cids
    }

    /// Run `request` and return the document it names, failing on an error.
    pub async fn write(&self, request: QueryRequest, field: &str) -> String {
        let response = self.node.execute_request(request).await;
        assert!(!response.has_errors(), "{field}: {:?}", response.errors);
        response.data.expect("data")[field][0]["_docID"]
            .as_str()
            .expect("document ID")
            .to_owned()
    }

    /// [`Self::write`], asked again while the node's provider reports
    /// itself unavailable or gives no answer in time. The permission check
    /// that fails so runs before the write, so a refused attempt changes
    /// nothing. An idle embedded deployment finalizes a block only every
    /// heartbeat, and on a loaded machine its evidence can come later than
    /// the request deadline, or its newest state past the light client's
    /// freshness limit, for a moment.
    pub async fn write_while_unavailable(&self, request: QueryRequest, field: &str) -> String {
        let deadline = Instant::now() + PATIENCE;
        loop {
            let response = self.node.execute_request(request.clone()).await;
            let unavailable = response.errors.iter().any(|error| {
                error.message.contains("ACP provider unavailable")
                    || error.message.contains("Vera: no answer within")
            });
            if !unavailable || Instant::now() >= deadline {
                assert!(!response.has_errors(), "{field}: {:?}", response.errors);
                return response.data.expect("data")[field][0]["_docID"]
                    .as_str()
                    .expect("document ID")
                    .to_owned();
            }
            pause().await;
        }
    }

    /// Whether `request` changed a document: false when it was refused with
    /// an error or found nothing it may change.
    pub async fn wrote(&self, request: QueryRequest, field: &str) -> bool {
        let response = self.node.execute_request(request).await;
        !response.has_errors()
            && response
                .data
                .and_then(|data| data[field].as_array().map(|rows| !rows.is_empty()))
                .unwrap_or(false)
    }

    /// Create a `Users` row as `actor`, signed with its key.
    pub async fn create(&self, actor: &Actor, name: &str) -> String {
        self.write(
            actor.signed(format!(
                "mutation {{ add_Users(input: {{name: \"{name}\", points: 0}}) {{ _docID }} }}"
            )),
            "add_Users",
        )
        .await
    }

    /// The mutation that adds `points` to `doc`.
    pub fn add_points(doc: &str, points: i64) -> String {
        format!(
            "mutation {{ update_Users(docID: \"{doc}\", input: {{points: {points}}}) {{ _docID }} }}"
        )
    }
}

/// One `Users` row as a reader sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub doc: String,
    pub name: Option<String>,
    pub points: Option<i64>,
}
