pub use hub_client::ACP_ADDRESS;
pub use hub_modules::acp::abi::IAcp;

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::FixedBytes;
    use alloy_sol_types::SolCall;

    fn pid() -> FixedBytes<32> {
        FixedBytes::from([7u8; 32])
    }

    #[test]
    fn set_relationship_subject_object_edge_round_trips() {
        let call = IAcp::setRelationshipSubjectCall {
            policyId: pid(),
            resource: "users".to_string(),
            objectId: "doc-1".to_string(),
            relation: "reader".to_string(),
            subjectKind: 2,
            subjectResource: "directory".to_string(),
            subjectObjectId: "d1".to_string(),
            subjectRelation: String::new(),
        };
        let encoded = call.abi_encode();
        assert_eq!(
            &encoded[..4],
            IAcp::setRelationshipSubjectCall::SELECTOR.as_slice()
        );

        let decoded = IAcp::setRelationshipSubjectCall::abi_decode(&encoded)
            .expect("object-edge call should decode");
        assert_eq!(decoded.policyId, pid());
        assert_eq!(decoded.resource, "users");
        assert_eq!(decoded.objectId, "doc-1");
        assert_eq!(decoded.relation, "reader");
        assert_eq!(decoded.subjectKind, 2);
        assert_eq!(decoded.subjectResource, "directory");
        assert_eq!(decoded.subjectObjectId, "d1");
        assert_eq!(decoded.subjectRelation, "");
    }

    #[test]
    fn delete_relationship_subject_userset_round_trips() {
        let call = IAcp::deleteRelationshipSubjectCall {
            policyId: pid(),
            resource: "users".to_string(),
            objectId: "doc-1".to_string(),
            relation: "reader".to_string(),
            subjectKind: 3,
            subjectResource: "directory".to_string(),
            subjectObjectId: "d1".to_string(),
            subjectRelation: "member".to_string(),
        };
        let encoded = call.abi_encode();
        assert_eq!(
            &encoded[..4],
            IAcp::deleteRelationshipSubjectCall::SELECTOR.as_slice()
        );

        let decoded = IAcp::deleteRelationshipSubjectCall::abi_decode(&encoded)
            .expect("userset call should decode");
        assert_eq!(decoded.subjectKind, 3);
        assert_eq!(decoded.subjectResource, "directory");
        assert_eq!(decoded.subjectObjectId, "d1");
        assert_eq!(decoded.subjectRelation, "member");
    }
}
