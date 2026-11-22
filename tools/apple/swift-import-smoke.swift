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
    var hex = DefraRemoteIdentityOptions()
    hex.key_type = keyType
    hex.public_key_hex = publicKeyHex
    hex.signer_handle = 1
    hex.sign = defraSwiftSmokeCallback
    let _ = defra_identity_new_remote(did, hex)

    var bytes = DefraRemoteIdentityOptions()
    bytes.key_type = keyType
    bytes.public_key = publicKeyBytes
    bytes.public_key_len = 65
    bytes.signer_handle = 1
    bytes.sign = defraSwiftSmokeCallback
    let _ = defra_identity_new_remote(did, bytes)

    let delegated = defra_identity_new_from_bearer_token(
        did, publicKeyHex, DefraIdentityBearerOptions())
    let _ = defra_node_set_identity(0, delegated.identity_ptr)
}
