import DefraFFI

let defraSwiftSmokeCallback: DefraRemoteSignCallback = { _, _, _, _, _, _, _, _ in
    return 0
}

let defraSwiftKeyringGet: DefraKeyringGetCallback = { _, _, _, _, outLen in
    outLen?.pointee = 0
    return DEFRA_KEYRING_NOT_FOUND
}

let defraSwiftKeyringSet: DefraKeyringSetCallback = { _, _, _, _ in
    return 0
}

let defraSwiftKeyringDelete: DefraKeyringDeleteCallback = { _, _ in
    return DEFRA_KEYRING_NOT_FOUND
}

func defraSwiftKeyringSmoke() {
    var options = DefraNodeInitOptions()
    options.peer_keyring_get = defraSwiftKeyringGet
    options.peer_keyring_set = defraSwiftKeyringSet
    options.peer_keyring_delete = defraSwiftKeyringDelete
    options.peer_keyring_context = 1
    let _ = defra_node_new(options)
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
