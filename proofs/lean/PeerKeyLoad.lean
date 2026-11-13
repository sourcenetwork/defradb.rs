/-!
# PeerKeyLoad: a start key is used as given and leaves the store untouched

The story, in plain words. A node's store remembers two things about its peer
key: a plaintext key it made itself, and the identity of a keyring key it was
told to use. Each start loads the key one of three ways: plain (the store's
own key, made on the first start), from a keyring (checked against the store,
whose identity is then recorded), or from a start key the caller hands in for
that one start, as Go's `P2PPrivateKey` is.

What must never happen: a start key is refused, or changes what the store
remembers. Either would make a per-start option leave its mark on every later
start.

This file proves, for one start at a time, that loading with a start key
answers that key (`start_key_is_used`) and hands the store back unchanged
(`start_key_leaves_store`), so a plain start after it loads exactly what it
would have loaded without it (`plain_after_start_key`). The sequence of
starts is the TLA+ model `proofs/tla/PeerKeyLoad.tla`.

Rust anchors: `crates/defra-node/src/peer_key.rs` `load_with_keyring`,
`load_or_create`, `StartPeerKey`.
-/

namespace PeerKeyLoad

-- What the store remembers about the peer key. Keys are named by numbers.
structure Store where
  -- The plaintext key the store made, or none.
  plain : Option Nat
  -- The keyring identity the store records, or none.
  pinned : Option Nat
  deriving DecidableEq

-- How one start finds its key.
inductive Source where
  -- No key option: the store's own key.
  | plain
  -- A keyring holding this key.
  | keyring (key : Nat)
  -- A key the caller hands in for this start only.
  | startKey (key : Nat)

-- Why a start is refused.
inductive Refusal where
  -- A keyring identity is recorded, so a keyring is required (`PeerKeyringRequired`).
  | keyringRequired
  -- The store holds a plaintext key a keyring cannot take over (`PlaintextPeerKey`).
  | plaintextKey
  -- The keyring holds another key than the one recorded (`KeyringIdentityMismatch`).
  | identityMismatch

-- One start: the key it uses and the store after it, or why it is refused.
-- `fresh` is the key a plain start makes when the store has none.
def load (store : Store) (fresh : Nat) : Source → Except Refusal (Nat × Store)
  -- A plain start: refused when a keyring identity is recorded, otherwise
  -- the store's key, made from `fresh` on the first start.
  | .plain =>
    if store.pinned.isSome then .error .keyringRequired
    else
      match store.plain with
      | some key => .ok (key, store)
      | none => .ok (fresh, { store with plain := some fresh })
  -- A keyring start: refused over a plaintext key or another recorded
  -- identity, otherwise the keyring's key, its identity recorded.
  | .keyring key =>
    if store.plain.isSome then .error .plaintextKey
    else if store.pinned.isSome ∧ store.pinned ≠ some key then .error .identityMismatch
    else .ok (key, { store with pinned := some key })
  -- A start key: used as given, the store untouched.
  | .startKey key => .ok (key, store)

-- A start with a start key always starts, with that key.
theorem start_key_is_used (store : Store) (fresh key : Nat) :
    (load store fresh (.startKey key)).map Prod.fst = .ok key := by
  -- The start key branch answers the key at once.
  rfl

-- A start with a start key hands the store back exactly as it found it.
theorem start_key_leaves_store (store : Store) (fresh key : Nat) :
    (load store fresh (.startKey key)).map Prod.snd = .ok store := by
  -- The start key branch returns the store it was given.
  rfl

-- A plain start after a start key loads exactly what it would have loaded
-- had the start key never been used: the start key changed nothing it reads.
theorem plain_after_start_key (store : Store) (fresh fresh' key : Nat) :
    (do
      let (_, after) ← load store fresh' (.startKey key)
      load after fresh .plain) = load store fresh .plain := by
  -- The start key answers the same store, so the plain start reads the same.
  rfl

end PeerKeyLoad
