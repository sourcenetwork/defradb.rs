use std::sync::Arc;

use crypto::PrivateKey;
use serde_json::json;

use super::{SigningWaveError, SigningWaves};

struct Session {
    private: Box<dyn PrivateKey>,
    waves: SigningWaves,
}

fn session() -> Session {
    let private = crypto::generate_ed25519().unwrap();
    let public = crypto::public_key_from_string(
        crypto::KeyType::Ed25519,
        &hex::encode(private.public_key().raw()),
    )
    .unwrap();
    Session {
        waves: SigningWaves::new(Arc::from(public)),
        private: Box::new(private),
    }
}

fn blocks(names: &[&str]) -> Vec<Arc<[u8]>> {
    names
        .iter()
        .map(|name| Arc::from(name.as_bytes()))
        .collect()
}

impl Session {
    fn sign(&self, wave: u64, payloads: &[Arc<[u8]>]) -> serde_json::Value {
        let signatures = payloads
            .iter()
            .map(|payload| hex::encode(self.private.sign(payload).unwrap()))
            .collect::<Vec<_>>();
        json!({"wave": wave, "signatures": signatures})
    }
}

#[test]
fn each_wave_is_signed_once_in_order() {
    let mut session = session();
    let children = blocks(&["name", "age"]);
    let composite = blocks(&["composite"]);
    assert_eq!(session.waves.open(&children), Ok(1));
    let answer = session.sign(1, &children);
    assert_eq!(session.waves.answer(answer).unwrap().len(), 2);
    assert_eq!(session.waves.open(&composite), Ok(2));
    let answer = session.sign(2, &composite);
    assert_eq!(session.waves.answer(answer).unwrap().len(), 1);
}

#[test]
fn a_wave_answered_twice_is_refused_and_the_session_stays_refused() {
    let mut session = session();
    let children = blocks(&["name"]);
    let composite = blocks(&["composite"]);
    session.waves.open(&children).unwrap();
    let answer = session.sign(1, &children);
    session.waves.answer(answer.clone()).unwrap();
    let refused = SigningWaveError::Resigned { wave: 1 };
    assert_eq!(session.waves.answer(answer.clone()), Err(refused.clone()));
    assert_eq!(refused.to_string(), "signing wave 1 was already signed");
    assert_eq!(session.waves.open(&composite), Err(refused));
}

#[test]
fn an_earlier_wave_answered_while_a_later_one_is_open_is_refused() {
    let mut session = session();
    let children = blocks(&["name"]);
    let composite = blocks(&["composite"]);
    session.waves.open(&children).unwrap();
    let first = session.sign(1, &children);
    session.waves.answer(first.clone()).unwrap();
    session.waves.open(&composite).unwrap();
    assert_eq!(
        session.waves.answer(first),
        Err(SigningWaveError::Resigned { wave: 1 })
    );
}

#[test]
fn a_wave_the_server_never_asked_for_is_outside_the_graph() {
    let mut session = session();
    let children = blocks(&["name"]);
    let answer = session.sign(1, &children);
    let refused = SigningWaveError::UnknownWave { wave: 1 };
    assert_eq!(session.waves.answer(answer), Err(refused.clone()));
    assert_eq!(
        refused.to_string(),
        "signing wave 1 is outside the approved graph"
    );
    assert_eq!(session.waves.open(&children), Err(refused));
}

#[test]
fn a_signature_over_another_block_is_outside_the_graph() {
    let mut session = session();
    let children = blocks(&["name", "age"]);
    session.waves.open(&children).unwrap();
    let answer = session.sign(1, &blocks(&["name", "other"]));
    let refused = SigningWaveError::OutsideGraph { wave: 1, index: 1 };
    assert_eq!(session.waves.answer(answer), Err(refused.clone()));
    assert_eq!(
        refused.to_string(),
        "signature 1 of signing wave 1 does not sign a block of the approved graph"
    );
}

#[test]
fn a_short_answer_is_refused() {
    let mut session = session();
    let children = blocks(&["name", "age"]);
    session.waves.open(&children).unwrap();
    let answer = session.sign(1, &children[..1]);
    assert_eq!(
        session.waves.answer(answer),
        Err(SigningWaveError::Incomplete {
            wave: 1,
            expected: 2,
            actual: 1
        })
    );
}

#[test]
fn an_answer_without_its_wave_number_is_malformed() {
    let mut session = session();
    let children = blocks(&["name"]);
    session.waves.open(&children).unwrap();
    let mut answer = session.sign(1, &children);
    answer.as_object_mut().unwrap().remove("wave");
    assert!(matches!(
        session.waves.answer(answer),
        Err(SigningWaveError::Malformed(_))
    ));
}

#[test]
fn a_block_signed_once_is_never_asked_again() {
    let mut session = session();
    let children = blocks(&["name"]);
    session.waves.open(&children).unwrap();
    let answer = session.sign(1, &children);
    session.waves.answer(answer).unwrap();
    let refused = SigningWaveError::BlockResigned { wave: 2, index: 0 };
    assert_eq!(session.waves.open(&children), Err(refused.clone()));
    assert_eq!(
        refused.to_string(),
        "block 0 of signing wave 2 was already signed in this session"
    );
}

#[test]
fn a_block_twice_in_one_wave_is_refused() {
    let mut session = session();
    assert_eq!(
        session.waves.open(&blocks(&["name", "name"])),
        Err(SigningWaveError::BlockResigned { wave: 1, index: 1 })
    );
}
