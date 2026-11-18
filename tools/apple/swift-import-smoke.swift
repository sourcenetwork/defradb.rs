import DefraFFI

let defraSwiftSmokeCallback: DefraRemoteSignCallback = { _, _, _, _, _, _, _, _ in
    return 0
}

func defraSwiftSmoke(
    did: UnsafePointer<CChar>,
    publicKeyHex: UnsafePointer<CChar>,
    publicKeyBytes: UnsafePointer<UInt8>,
    keyType: UnsafePointer<CChar>
) {
    let _ = defra_identity_new_remote(did, publicKeyHex, keyType, 1, defraSwiftSmokeCallback)
    let _ = defra_identity_new_remote_bytes(
        did,
        publicKeyBytes,
        65,
        keyType,
        1,
        defraSwiftSmokeCallback
    )
    let delegated = defra_identity_new_from_bearer_token(did, publicKeyHex)
    let _ = defra_node_set_identity(0, delegated.identity_ptr)
}
