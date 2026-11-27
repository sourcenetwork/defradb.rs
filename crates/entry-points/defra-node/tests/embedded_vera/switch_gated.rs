//! A document a backup creates in a collection whose policy gates creates:
//! its registration carries the relationships the node derived from the
//! document's relation fields, stays owed while the device is a backup, and
//! settles with them once the device takes the deployment over.

use acp::{DocumentACP as _, DocumentPermission, Identity};
use defra_node::{EmbeddedVeraDevice, Node, QueryRequest};

use super::switching::{certify, device, eventually, identity, pair, until_settled, vera};
use super::STRANGER;

/// A book's author is an `author` the creator must be allowed to publish
/// for, and a reader of that author reads the book.
const LIBRARY: &str = "\
name: library
resources:
  - name: author
    permissions:
      - name: read
        expr: writer + reader
      - name: update
        expr: writer
      - name: delete
        expr: writer
      - name: publish
        expr: writer
    relations:
      - name: writer
        types:
          - actor
      - name: reader
        types:
          - actor
  - name: book
    permissions:
      - name: read
        expr: author->read
      - name: update
        expr: editor
      - name: delete
        expr: editor
    relations:
      - name: author
        types:
          - author
      - name: editor
        types:
          - actor
    collection:
      create: author->publish
";

fn library(policy: &str) -> String {
    format!(
        r#"
        type Author @policy(id: "{policy}", resource: "author") {{
            name: String
        }}
        type Book @governed(root: "library") @policy(id: "{policy}", resource: "book") {{
            title: String
            author: Author @immutable @policy(relation: "author")
        }}
        "#
    )
}

fn add(collection: &str, input: &str, creator: &identity::Did) -> QueryRequest {
    QueryRequest::new(format!(
        "mutation {{ add_{collection}(input: {{{input}}}) {{ _docID }} }}"
    ))
    .with_identity(Some(creator.clone()))
}

fn created(response: defra_node::QueryResponse, collection: &str) -> String {
    assert!(!response.has_errors(), "{:?}", response.errors);
    response.data.expect("data")[format!("add_{collection}")][0]["_docID"]
        .as_str()
        .expect("document ID")
        .to_owned()
}

/// Whether `who` reads `doc` of `resource`, decided from the node's own
/// verified Vera state with nothing it still owes taken into account.
async fn chain_reads(
    node: &Node,
    who: &identity::Did,
    policy: &str,
    resource: &str,
    doc: &str,
) -> bool {
    node.vera_acp()
        .expect("vera.rs document ACP")
        .check_doc_access(
            &Identity::Authenticated(who.clone()),
            DocumentPermission::Read,
            policy,
            resource,
            doc,
        )
        .await
        .expect("access decision")
}

fn owner(node: &Node) -> identity::Did {
    identity::Did::new(node.vera_acp().unwrap().self_did().unwrap()).unwrap()
}

async fn owed(node: &Node, actor: &identity::Did) -> Vec<acp::OwedEntry> {
    node.with_identity(actor.clone())
        .owed_acp(actor.as_str())
        .await
        .expect("what is owed")
}

#[test]
#[mantra_macros::req_test("defra.acp.vera_embedded_switching")]
fn a_gated_create_on_a_backup_settles_with_its_relationships_after_a_take_over() {
    workers::block_on(async {
        let directories = [tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap()];
        let [laptop_device, phone_device] = pair();
        let deployment = identity().deal(laptop_device.listen).expect("deployment");
        let laptop = device(
            directories[0].path(),
            42,
            laptop_device.listen,
            EmbeddedVeraDevice::Active {
                deployment: Box::new(deployment.clone()),
                switching: laptop_device.switching(),
            },
        )
        .await;
        let phone = device(
            directories[1].path(),
            43,
            phone_device.listen,
            EmbeddedVeraDevice::Backup {
                record: deployment.record().to_vec(),
                active: laptop_device.listen,
                switching: phone_device.switching(),
            },
        )
        .await;

        // On the active laptop: the gated collection, an author the phone's
        // actor may publish for, and a stranger who reads that author.
        let laptop_owner = owner(&laptop);
        let phone_owner = owner(&phone);
        let stranger = identity::Did::new(STRANGER).unwrap();
        let policy = laptop
            .vera_acp()
            .unwrap()
            .add_policy(laptop_owner.as_str(), LIBRARY, None)
            .await
            .expect("policy");
        laptop
            .add_collection(&library(&policy))
            .await
            .expect("collection");
        let author = created(
            laptop
                .execute_request(add("Author", r#"name: "Mike""#, &laptop_owner))
                .await,
            "Author",
        );
        eventually("the laptop settles the author", || async {
            owed(&laptop, &laptop_owner).await.is_empty()
        })
        .await;
        for (relation, who) in [("writer", &phone_owner), ("reader", &stranger)] {
            laptop
                .document_acp()
                .add_actor_relationship(
                    &laptop_owner,
                    who,
                    &policy,
                    "author",
                    &author,
                    relation,
                    &[],
                )
                .await
                .expect("relationship");
        }

        // The phone, a backup, verifies all of it from its own Vera node.
        eventually("the phone reads the laptop's policy", || async {
            phone.add_collection(&library(&policy)).await.is_ok()
        })
        .await;
        eventually("the phone verifies the author's reader", || async {
            chain_reads(&phone, &stranger, &policy, "author", &author).await
        })
        .await;

        // The gate admits the phone's create from certified state; the
        // backup takes no submission, so the registration and the
        // relationship derived from the book's author stay owed.
        let book = created(
            phone
                .execute_request(add(
                    "Book",
                    &format!(r#"title: "Dune", author: "{author}""#),
                    &phone_owner,
                ))
                .await,
            "Book",
        );
        let waiting = owed(&phone, &phone_owner).await;
        assert!(
            waiting
                .iter()
                .any(|entry| entry.what == acp::OwedWhat::Registration && entry.object == book),
            "the book's registration is owed on the backup: {waiting:?}"
        );
        assert!(!chain_reads(&phone, &stranger, &policy, "book", &book).await);

        // The laptop stops without a handover; the phone takes over and
        // sends what it owed, relationships included.
        laptop.shutdown().await.expect("laptop shutdown");
        drop(laptop);
        let change = until_settled(|| vera(&phone).takeover_change()).await;
        phone
            .take_over_embedded_vera(deployment.keys().clone(), certify(change))
            .await
            .expect("take-over");
        eventually("the phone settles the book", || async {
            owed(&phone, &phone_owner).await.is_empty()
        })
        .await;
        assert!(
            chain_reads(&phone, &stranger, &policy, "book", &book).await,
            "a reader of the author reads the book through the derived relationship"
        );
        assert_eq!(
            phone
                .vera_acp()
                .unwrap()
                .get_doc_owner(&policy, "book", &book)
                .await
                .expect("owner"),
            Some(phone_owner)
        );
        phone.shutdown().await.expect("phone shutdown");
    })
    .unwrap();
}
