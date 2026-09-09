use super::*;
use crate::proto::{
    StartSignResponse,
    sign_service_server::{SignService, SignServiceServer},
};
use std::collections::HashSet;
use std::sync::Mutex;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{Request, Response, Status};

const DERIVATION: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const DST: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_NUL_";

fn key(seed: u8) -> blst::min_pk::SecretKey {
    blst::min_pk::SecretKey::key_gen(&[seed; 32], &[]).unwrap()
}

fn identity() -> Arc<identity::RawIdentity> {
    Arc::new(
        identity::RawIdentity::from_ed25519(
            crypto::Ed25519PrivateKey::from_bytes(
                &crypto::ed25519_key_from_seed(&[3; 32]).unwrap(),
            )
            .unwrap(),
        )
        .unwrap(),
    )
}

struct Service {
    identity: Arc<identity::RawIdentity>,
    response: Option<String>,
    tokens: Arc<Mutex<HashSet<String>>>,
}

#[tonic::async_trait]
impl SignService for Service {
    async fn start_sign(
        &self,
        request: Request<StartSignRequest>,
    ) -> Result<Response<StartSignResponse>, Status> {
        let token = request
            .metadata()
            .get("authorization")
            .unwrap()
            .to_str()
            .unwrap()
            .strip_prefix("Bearer ")
            .unwrap();
        let parts: Vec<_> = token.split('.').collect();
        let claims: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
        assert!(
            self.identity
                .pub_key()
                .verify(
                    format!("{}.{}", parts[0], parts[1]).as_bytes(),
                    &URL_SAFE_NO_PAD.decode(parts[2]).unwrap(),
                )
                .unwrap()
        );
        assert_eq!(claims["iss"], self.identity.did().unwrap().to_string());
        assert!(claims.get("sub").is_none());
        assert_eq!(
            claims["exp"].as_u64().unwrap() - claims["iat"].as_u64().unwrap(),
            300
        );
        assert!(
            self.tokens
                .lock()
                .unwrap()
                .insert(claims["jti"].as_str().unwrap().to_owned())
        );
        let request = request.into_inner();
        assert_eq!(request.derivation_id, DERIVATION);
        assert_eq!(claims["derivation_id"], DERIVATION);
        assert_eq!(
            claims["message_sha256"],
            serde_json::json!(Sha256::digest(&request.message).to_vec())
        );
        Ok(Response::new(StartSignResponse {
            status: "completed".into(),
            message: String::new(),
            created_at: 0,
            signature: self
                .response
                .clone()
                .unwrap_or_else(|| hex::encode(key(7).sign(&request.message, DST, &[]).compress())),
        }))
    }
}

async fn client(
    response: Option<String>,
) -> (
    OrbisClient,
    tokio::task::JoinHandle<()>,
    Arc<Mutex<HashSet<String>>>,
) {
    let identity = identity();
    let tokens = Arc::new(Mutex::new(HashSet::new()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let service = Service {
        identity: identity.clone(),
        response,
        tokens: tokens.clone(),
    };
    let task = tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(SignServiceServer::new(service))
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
    let client = tokio::task::spawn_blocking(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(OrbisClient::new(
                endpoint,
                DERIVATION.into(),
                key(7).sk_to_pk().compress().to_vec(),
                identity,
            ))
    })
    .await
    .unwrap()
    .unwrap();
    (client, task, tokens)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn current_protocol_signs_with_distinct_message_bound_tokens() {
    let (client, task, tokens) = client(None).await;
    for message in [b"first".as_slice(), b"second"] {
        let signature = client.sign_sync(message, None).unwrap();
        assert!(client.public_key.verify(message, &signature).unwrap());
    }
    assert_eq!(tokens.lock().unwrap().len(), 2);
    let authorization = SigningAuthorization::Decision {
        decision_id: "decision".into(),
    };
    assert!(
        client
            .sign_sync(b"message", Some(&authorization))
            .unwrap_err()
            .contains("overrides")
    );
    assert!(
        client
            .sign_sync(&vec![0; 1024 * 1024 + 1], None)
            .unwrap_err()
            .contains("1 MiB")
    );
    assert_eq!(tokens.lock().unwrap().len(), 2);
    drop(client);
    task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn current_protocol_rejects_invalid_signatures() {
    for signature in [
        String::new(),
        "not hex".into(),
        "010203".into(),
        hex::encode(key(7).sign(b"wrong message", DST, &[]).compress()),
        hex::encode(key(8).sign(b"message", DST, &[]).compress()),
    ] {
        let (client, task, _) = client(Some(signature)).await;
        assert!(client.sign_sync(b"message", None).is_err());
        drop(client);
        task.abort();
    }
}

#[tokio::test]
async fn invalid_configuration_fails_before_connecting() {
    let pk = key(7).sk_to_pk().compress().to_vec();
    for derivation in ["", "bad", &"A".repeat(64)] {
        let result = OrbisClient::new(
            "not an endpoint".into(),
            derivation.into(),
            pk.clone(),
            identity(),
        )
        .await;
        assert!(result.err().unwrap().to_string().contains("derivation ID"));
    }
    let secp = Arc::new(
        identity::RawIdentity::from_secp256k1(
            crypto::Secp256k1PrivateKey::from_bytes(&[4; 32]).unwrap(),
        )
        .unwrap(),
    );
    let result = OrbisClient::new("not an endpoint".into(), DERIVATION.into(), pk, secp).await;
    assert!(result.err().unwrap().to_string().contains("Ed25519"));
}
