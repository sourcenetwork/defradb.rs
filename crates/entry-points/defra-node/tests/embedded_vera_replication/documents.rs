//! Reading and writing the `Users` collection on a peer, and asking its
//! Vera node about the documents in it.

use std::time::Instant;

use acp::DocumentACP as _;
use defra_node::{DocumentPermission, QueryRequest, QueryResponse};
use identity::Did;

use super::actor::Actor;
use super::peer::Peer;
use super::wait::{pause, PATIENCE};

impl Peer {
    /// `actor`'s permission on `doc` as this peer's Vera node answers it,
    /// or the error it gives instead.
    pub async fn may(
        &self,
        actor: &Did,
        permission: DocumentPermission,
        doc: &str,
    ) -> acp::Result<bool> {
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
    }

    /// [`Self::may`], asked again while the provider is unavailable, failing
    /// on any other error: an answer, never an error read as a denial.
    pub async fn answer(&self, actor: &Did, permission: DocumentPermission, doc: &str) -> bool {
        let deadline = Instant::now() + PATIENCE;
        loop {
            match self.may(actor, permission, doc).await {
                Ok(allowed) => return allowed,
                Err(error) if transient(&error.to_string()) && Instant::now() < deadline => {
                    pause().await
                }
                Err(error) => panic!("{permission:?} on {doc} for {actor}: {error}"),
            }
        }
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

    /// The `Users` rows `reader` sees, by document ID. The query is asked
    /// again while the provider is unavailable, and fails on any other
    /// error. A row the provider could not decide is left out without an
    /// error, as the permission filter fails closed.
    pub async fn rows(&self, reader: &Did) -> Vec<Row> {
        let request = QueryRequest::new("{ Users { _docID name points } }")
            .with_identity(Some(reader.clone()));
        let response = self.answered(request).await;
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

    /// The points of `doc`, which `reader` may read: read again while the
    /// row is left out, as it is for a moment when the provider cannot
    /// decide it, failing after [`PATIENCE`]. A value read is returned at
    /// once, so a wrong one fails its check at once.
    pub async fn visible_points(&self, reader: &Did, doc: &str) -> i64 {
        let deadline = Instant::now() + PATIENCE;
        loop {
            if let Some(points) = self.points(reader, doc).await {
                return points;
            }
            assert!(
                Instant::now() < deadline,
                "{reader} cannot read the points of {doc}"
            );
            pause().await;
        }
    }

    /// The composite commits this peer holds merged for `doc`, sorted. The
    /// query decides the history's access through the node's provider, and
    /// is asked again while the provider is unavailable.
    pub async fn commits(&self, reader: &Did, doc: &str) -> Vec<String> {
        let request = QueryRequest::new(format!(
            "{{ _commits(docID: \"{doc}\", filter: {{fieldName: {{_eq: \"_C\"}}}}) {{ cid }} }}"
        ))
        .with_identity(Some(reader.clone()));
        let response = self.answered(request).await;
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

    /// The response to `request`, asked again while it fails only because
    /// the provider is unavailable, until [`PATIENCE`] has passed.
    async fn answered(&self, request: QueryRequest) -> QueryResponse {
        let deadline = Instant::now() + PATIENCE;
        loop {
            let response = self.node.execute_request(request.clone()).await;
            if !unavailable(&response) || Instant::now() >= deadline {
                return response;
            }
            pause().await;
        }
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
        let response = self.answered(request).await;
        assert!(!response.has_errors(), "{field}: {:?}", response.errors);
        response.data.expect("data")[field][0]["_docID"]
            .as_str()
            .expect("document ID")
            .to_owned()
    }

    /// Run `request`, asked again while the provider is unavailable, and
    /// return what it changed: the documents it names, or the errors it
    /// was refused with.
    pub async fn attempt(
        &self,
        request: QueryRequest,
        field: &str,
    ) -> Result<Vec<String>, Vec<String>> {
        let response = self.answered(request).await;
        if response.has_errors() {
            return Err(response
                .errors
                .iter()
                .map(|error| error.message.clone())
                .collect());
        }
        Ok(response.data.expect("data")[field]
            .as_array()
            .expect("documents")
            .iter()
            .map(|row| row["_docID"].as_str().expect("document ID").to_owned())
            .collect())
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

/// Whether `message` says the provider could not answer: it was unavailable,
/// or gave no answer in time. Nothing is decided, so the same request may be
/// made again.
pub fn transient(message: &str) -> bool {
    message.contains("ACP provider unavailable") || message.contains("Vera: no answer within")
}

fn unavailable(response: &QueryResponse) -> bool {
    response
        .errors
        .iter()
        .any(|error| transient(&error.message))
}

/// One `Users` row as a reader sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub doc: String,
    pub name: Option<String>,
    pub points: Option<i64>,
}
