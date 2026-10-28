use super::*;
use crate::proto::{
    sign_service_server::{SignService, SignServiceServer},
    StartSignResponse,
};
use regolith::sync::HopscotchMap;
use tonic::{Request, Response, Status};

const DERIVATION: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const DST: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_AUG_";

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

type Tokens = HopscotchMap<String, (), rapidhash::fast::RandomState>;

struct Service {
    identity: Arc<identity::RawIdentity>,
    response: Option<String>,
    tokens: Arc<Tokens>,
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
        assert!(self
            .identity
            .pub_key()
            .verify(
                format!("{}.{}", parts[0], parts[1]).as_bytes(),
                &URL_SAFE_NO_PAD.decode(parts[2]).unwrap(),
            )
            .unwrap());
        assert_eq!(claims["iss"], self.identity.did().unwrap().to_string());
        assert!(claims.get("sub").is_none());
        assert_eq!(
            claims["exp"].as_u64().unwrap() - claims["iat"].as_u64().unwrap(),
            300
        );
        assert!(self
            .tokens
            .insert_if_absent(claims["jti"].as_str().unwrap().to_owned(), ())
            .is_none());
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
            signature: self.response.clone().unwrap_or_else(|| {
                hex::encode(
                    key(7)
                        .sign(&request.message, DST, &key(7).sk_to_pk().to_bytes())
                        .compress(),
                )
            }),
        }))
    }
}

async fn client(response: Option<String>) -> (OrbisClient, nuclei::Task<()>, Arc<Tokens>) {
    let identity = identity();
    let tokens = Arc::new(Tokens::with_hasher(rapidhash::fast::RandomState::default()));
    let listener = nuclei::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.get_ref().local_addr().unwrap());
    let service = Service {
        identity: identity.clone(),
        response,
        tokens: tokens.clone(),
    };
    let task = nuclei::spawn(async move {
        net::server::serve(listener, None, SignServiceServer::new(service))
            .await
            .unwrap();
    });
    let client = OrbisClient::new(
        endpoint,
        DERIVATION.into(),
        key(7).sk_to_pk().compress().to_vec(),
        identity,
        edge::testing::edge(),
    )
    .await
    .unwrap();
    (client, task, tokens)
}

#[nuclei::test(threads = 2)]
async fn current_protocol_signs_with_distinct_message_bound_tokens() {
    let (client, task, tokens) = client(None).await;
    for message in [b"first".as_slice(), b"second"] {
        let signature = client.sign_sync(message, None).unwrap();
        assert!(client.public_key.verify(message, &signature).unwrap());
    }
    assert_eq!(tokens.len(), 2);
    let authorization = SigningAuthorization::Decision {
        decision_id: "decision".into(),
    };
    assert!(client
        .sign_sync(b"message", Some(&authorization))
        .unwrap_err()
        .contains("overrides"));
    assert!(client
        .sign_sync(&vec![0; 1024 * 1024 + 1], None)
        .unwrap_err()
        .contains("1 MiB"));
    assert_eq!(tokens.len(), 2);
    drop(client);
    task.abort();
}

#[nuclei::test(threads = 2)]
async fn current_protocol_rejects_invalid_signatures() {
    for signature in [
        hex::encode(
            key(7)
                .sign(
                    b"message",
                    b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_NUL_",
                    &[],
                )
                .compress(),
        ),
        String::new(),
        "not hex".into(),
        "010203".into(),
        hex::encode(
            key(7)
                .sign(b"wrong message", DST, &key(7).sk_to_pk().to_bytes())
                .compress(),
        ),
        hex::encode(
            key(8)
                .sign(b"message", DST, &key(8).sk_to_pk().to_bytes())
                .compress(),
        ),
    ] {
        let (client, task, _) = client(Some(signature)).await;
        assert!(client.sign_sync(b"message", None).is_err());
        drop(client);
        task.abort();
    }
}

#[nuclei::test]
async fn invalid_configuration_fails_before_connecting() {
    let pk = key(7).sk_to_pk().compress().to_vec();
    for derivation in ["", "bad", &"A".repeat(64)] {
        let result = OrbisClient::new(
            "not an endpoint".into(),
            derivation.into(),
            pk.clone(),
            identity(),
            edge::testing::edge(),
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
    let result = OrbisClient::new(
        "not an endpoint".into(),
        DERIVATION.into(),
        pk,
        secp,
        edge::testing::edge(),
    )
    .await;
    assert!(result.err().unwrap().to_string().contains("Ed25519"));
}

#[nuclei::test(threads = 2)]
async fn signs_from_a_thread_outside_any_runtime() {
    let (client, task, tokens) = client(None).await;
    let client = Arc::new(client);
    let signer = Arc::clone(&client);

    let signature = std::thread::spawn(move || signer.sign_sync(b"plain thread", None))
        .join()
        .unwrap()
        .unwrap();

    assert!(client
        .public_key
        .verify(b"plain thread", &signature)
        .unwrap());
    assert_eq!(tokens.len(), 1);
    task.abort();
}
