/-!
# GossipFrame.RawOrigin: a sealed raw gossip payload opens to the node that sealed it

The story, in plain words. On iroh, gossip tells a node only which neighbour
handed it a message, not who first sent it. A request that hops through a
relay would then be answered, and checked for permission, as the relay. So a
sender seals each raw payload: a fixed six-byte marker, its 32-byte public
key, a 64-byte signature, then the payload. The receiver checks the marker,
reads the key and signature back, and accepts the payload as that key's only
if the signature checks out. A payload without the marker came from a node
that predates sealing and is passed through untouched.

What must never happen:
* a sealed payload opens to a different key or a different payload
  (`open_seal`);
* a sealed payload is mistaken for an unsealed one, which would let a relay
  stand in for its sender (`open_seal_never_unsigned`);
* an unsealed payload is altered or refused (`open_unsigned`);
* a marked payload too short to hold a key and a signature is accepted
  (`open_refuses_short`).

Bytes are modelled as natural numbers below 256. The signature check is a
parameter, `verify`: the proof holds for any check, and says nothing about
the cryptography itself.

Rust anchors: `seal`, `open` in `crates/p2p/src/iroh/gossip/raw_origin.rs`;
`raw_publisher` in `crates/p2p/src/iroh/gossip/delivery.rs`.
-/

namespace GossipFrame.RawOrigin

-- The marker a sealed payload starts with: 0xFF, then "DRAW", then version 1.
def magic : List Nat := [255, 68, 82, 65, 87, 1]

-- Seal: the marker, the sender's key, its signature, then the payload.
def sealRaw (key sig payload : List Nat) : List Nat := magic ++ key ++ sig ++ payload

-- What a receiver makes of a payload.
inductive Opened where
  -- sealed by `key`, whose signature holds, around `payload`;
  | signed (key payload : List Nat)
  -- not sealed: the payload as it arrived;
  | unsigned (payload : List Nat)
  -- marked as sealed, but short or with a signature that fails.
  | refused

-- The bytes after the marker, or nothing if the payload does not start with it.
def stripMagic (bytes : List Nat) : Option (List Nat) :=
  -- The first six bytes are the marker: keep what follows;
  if bytes.take 6 = magic then some (bytes.drop 6)
  -- otherwise there is no marker.
  else none

-- Open a payload, with `verify key sig payload` as the signature check.
def openRaw (verify : List Nat → List Nat → List Nat → Bool) (bytes : List Nat) : Opened :=
  match stripMagic bytes with
  -- No marker: an unsealed payload, passed on as it is.
  | none => .unsigned bytes
  -- A marker: read the key and signature, refuse if they are not all there,
  | some rest =>
    if rest.length < 96 then .refused
    -- and accept the rest as the key's payload only if the signature holds.
    else if verify (rest.take 32) ((rest.drop 32).take 64) (rest.drop 96) then
      .signed (rest.take 32) (rest.drop 96)
    else .refused

-- The marker is six bytes long.
theorem magic_length : magic.length = 6 := rfl

-- Promise: a sealed payload's marker is found, and what follows it is the key,
-- the signature and the payload, unchanged.
theorem stripMagic_seal (key sig payload : List Nat) :
    stripMagic (sealRaw key sig payload) = some (key ++ sig ++ payload) := by
  -- The first six bytes of the seal are the marker, the rest is what follows.
  simp [stripMagic, sealRaw, List.append_assoc, magic]

-- Promise: a payload sealed with a 32-byte key and a 64-byte signature that
-- checks out opens to exactly that key and that payload.
theorem open_seal (verify : List Nat → List Nat → List Nat → Bool)
    (key sig payload : List Nat) (hk : key.length = 32) (hs : sig.length = 64)
    (hv : verify key sig payload = true) :
    openRaw verify (sealRaw key sig payload) = .signed key payload := by
  -- The marker comes off, leaving key, signature and payload.
  simp only [openRaw, stripMagic_seal]
  -- Reading 32 bytes gives back the key, since the key is exactly 32 long;
  have hkey : (key ++ sig ++ payload).take 32 = key := by
    simp [List.append_assoc, List.take_append_eq_append_take, hk]
  -- the next 64 bytes are the signature, since it is exactly 64 long;
  have hsig : ((key ++ sig ++ payload).drop 32).take 64 = sig := by
    simp [List.append_assoc, List.drop_append_eq_append_drop, hk,
      List.take_append_eq_append_take, hs]
  -- and what is left after 96 bytes is the payload.
  have hpay : (key ++ sig ++ payload).drop 96 = payload := by
    have h96 : (96 : Nat) = 32 + 64 := rfl
    rw [h96, ← List.drop_drop]
    simp [List.append_assoc, List.drop_append_eq_append_drop, hk, hs]
  -- The seal is at least 96 bytes, so it is not refused as short.
  have hlen : ¬ (key ++ sig ++ payload).length < 96 := by
    simp only [List.length_append, hk, hs]
    omega
  -- With all of that, the signature check passes and the result is the key's payload.
  rw [if_neg hlen, hkey, hsig, hpay, if_pos hv]

-- Promise: whatever the signature check says, a sealed payload is never passed
-- on as unsealed, so a relay can never be taken for its sender.
theorem open_seal_never_unsigned (verify : List Nat → List Nat → List Nat → Bool)
    (key sig payload p : List Nat) :
    openRaw verify (sealRaw key sig payload) ≠ .unsigned p := by
  -- The marker is always found on a seal, so only "signed" or "refused" can come back.
  simp only [openRaw, stripMagic_seal]
  -- Each remaining branch is a different kind of result from "unsigned".
  split <;> (try split) <;> simp

-- Promise: a payload that does not start with 0xFF is passed on untouched.
theorem open_unsigned (verify : List Nat → List Nat → List Nat → Bool)
    (bytes : List Nat) (h : bytes.head? ≠ some 255) :
    openRaw verify bytes = .unsigned bytes := by
  -- Such a payload cannot start with the marker, whose first byte is 0xFF.
  have hnone : stripMagic bytes = none := by
    -- Look at the payload byte by byte.
    cases bytes with
    -- An empty payload has no marker;
    | nil => simp [stripMagic, magic]
    -- a non-empty one starts with a byte other than 0xFF, unlike the marker.
    | cons b rest =>
      simp only [List.head?_cons, ne_eq, Option.some.injEq] at h
      simp [stripMagic, magic, h]
  -- With no marker, the payload comes back as it arrived.
  simp [openRaw, hnone]

-- Promise: a payload that carries the marker but is too short to hold a key
-- and a signature is refused, never passed on.
theorem open_refuses_short (verify : List Nat → List Nat → List Nat → Bool)
    (rest : List Nat) (h : rest.length < 96) :
    openRaw verify (magic ++ rest) = .refused := by
  -- The marker comes off, leaving the short remainder,
  have hstrip : stripMagic (magic ++ rest) = some rest := by
    simp [stripMagic, magic]
  -- which is under 96 bytes and so refused.
  simp [openRaw, hstrip, h]

end GossipFrame.RawOrigin
