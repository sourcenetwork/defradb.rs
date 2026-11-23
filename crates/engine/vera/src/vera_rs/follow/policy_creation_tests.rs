//! A lost policy creation is named, not just counted.

use crate::vera_rs::abi::{IAcp, ACP_ADDRESS};
use crate::vera_rs::did_spelling;
use crate::vera_rs::follow::effect::{effect_of, Effect};
use crate::vera_rs::follow::fixtures::{token, OWNER};
use alloy_sol_types::SolCall as _;
use vera_client::BlsSigner;
use vera_domain::NativeTx;
use vera_modules::acp::types::PolicyMarshalingType as WireMarshalingType;

fn creation_wire() -> Vec<u8> {
    let policy = "name: taken\nresources: []\n";
    let nonce = [3u8; 32];
    let call = IAcp::bearerCreatePolicyCall {
        bearerToken: token(OWNER),
        policy: policy.as_bytes().to_vec().into(),
        marshalType: WireMarshalingType::ShortYaml as _,
        nonce: nonce.into(),
    };
    let calldata = call.abi_encode();
    BlsSigner::new(7u64.into(), 9001)
        .unwrap()
        .sign_native_tx_with_sequence(ACP_ADDRESS, calldata.into(), 4)
        .unwrap()
}

#[test]
fn a_lost_policy_creation_names_its_derived_id() {
    let wire = creation_wire();
    let tx = NativeTx::decode_wire(&wire).unwrap();
    let call = IAcp::bearerCreatePolicyCall::abi_decode(&tx.calldata).unwrap();
    let policy = String::from_utf8_lossy(&call.policy).into_owned();
    let creator = did_spelling::defra(OWNER).into_owned();
    let expected =
        crate::vera_rs::predict_policy_id(&policy, crate::PolicyNonce(call.nonce.0), &creator)
            .unwrap();
    assert_eq!(
        effect_of(&wire),
        Effect::PolicyCreation {
            policy_id: expected
        }
    );
}
