//! A policy nonce as text: 64 hexadecimal characters, and nothing else.

use acp::{PolicyNonce, PolicyNonceError};

const TEXT: &str = "00ff10a0000000000000000000000000000000000000000000000000000000fe";

#[test]
fn a_nonce_written_out_reads_back_as_itself() {
    let nonce: PolicyNonce = TEXT.parse().unwrap();
    assert_eq!(nonce.0[..4], [0x00, 0xff, 0x10, 0xa0]);
    assert_eq!(nonce.0[31], 0xfe);
    assert_eq!(nonce.to_string(), TEXT);
    assert_eq!(
        TEXT.to_uppercase().parse::<PolicyNonce>().unwrap(),
        nonce,
        "either case reads as one nonce"
    );
}

#[test]
fn text_of_another_length_or_with_other_characters_is_not_a_nonce() {
    for text in [
        "",
        &TEXT[..62],
        &format!("{TEXT}00"),
        &TEXT.replacen('0', "g", 1),
        &format!("0x{}", &TEXT[2..]),
    ] {
        assert_eq!(text.parse::<PolicyNonce>(), Err(PolicyNonceError), "{text}");
    }
}

#[test]
fn a_nonce_is_serialized_as_its_text() {
    let nonce: PolicyNonce = TEXT.parse().unwrap();
    let json = serde_json::to_string(&nonce).unwrap();
    assert_eq!(json, format!("\"{TEXT}\""));
    assert_eq!(serde_json::from_str::<PolicyNonce>(&json).unwrap(), nonce);
    assert!(serde_json::from_str::<PolicyNonce>("\"00\"").is_err());
}
