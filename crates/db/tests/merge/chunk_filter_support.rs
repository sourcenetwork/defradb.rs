//! What the chunk filtering tests share: an article schema with a chunked
//! body, and a replicator that takes only public articles.

use db::DB;
use storage::RegolithStore;

/// `kind` is `@immutable`, as every replication filter field must be.
pub const SDL: &str = "type Article { kind: String @immutable body: String @chunk(maxChars: 64) }";
pub const CHUNKS: &str = "Article__body_chunk";
pub const PUBLIC: &str = "public";
pub const SECRET: &str = "secret";
pub const PEER: &str = "receiver";

/// A replicator for `PEER` that takes only public articles, with their chunks.
pub fn public_only_replicator(db: &DB<RegolithStore>) -> p2p::ReplicatorInfo {
    let id_of = |name: &str| {
        db.get_collection(name)
            .unwrap()
            .unwrap()
            .collection_id()
            .to_string()
    };
    let (articles, chunks) = (id_of("Article"), id_of(CHUNKS));
    p2p::ReplicatorInfo::from_raw_with_filters(
        PEER.to_string(),
        vec![articles.clone(), chunks.clone()],
        vec![],
        [(
            articles.clone(),
            p2p::ReplicationFilter::eq("kind", serde_json::json!(PUBLIC)),
        )]
        .into(),
    )
    .with_chunk_sources([(chunks, articles)])
}
