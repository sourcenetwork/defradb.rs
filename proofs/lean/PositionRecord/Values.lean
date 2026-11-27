import PositionRecord.Codec

/-!
# PositionRecord.Values: a record of field values reads back as its keys

The story, in plain words. A position record lives under a key that already
names the collection, the index and the document. Every entry key of that
document in that index starts with the same bytes (the index's prefix) and
ends with the same bytes (the document's suffix); only the field values in
between differ. So a record keeps just those values. It starts with the byte
1, then gives each entry's values behind their length: one byte for a length
below 255, else the byte 255 and the length in four bytes. Reading it puts
the prefix and the suffix back around each value.

Records written before kept whole keys, each behind a four-byte length (the
`encode` of `PositionRecord.Codec`). Such a record is still read: it is empty
or starts with the byte 0, because regolith refuses every key longer than its
`DEFAULT_MAX_KEY_SIZE` of 8 MiB and the store caps every open at that limit,
so it can never be taken for a record of values.

What must never happen: a record of values reads back as different keys
(`decodeRecord_encodeValues`); a record of whole keys is read as a record of
values, or reads back as different keys (`decodeRecord_encode`). If either
did, the drain would delete the wrong entries and leave the real ones behind.

Bytes are modelled as natural numbers below 256.

Rust anchors: `encode_record`, `decode_record`, `Frame` in
`crates/database/db/src/index/deferred/record.rs`.
-/

namespace PositionRecord

-- The first byte of a record that keeps field values (`VALUES`).
def valuesTag : Nat := 1

-- The length byte that says four more bytes give the length (`LONG`).
def long : Nat := 255

-- A length: one byte below 255, else 255 and four big-endian bytes (`push_len`).
def lenBytes (n : Nat) : List Nat :=
  -- A short length is its own byte; a long one is flagged, then spelled out.
  if n < long then [n] else long :: be32 n

-- Read a length back (`read_len`); nothing if the bytes run out.
def readLen : List Nat → Option (Nat × List Nat)
  -- No bytes: a broken record.
  | [] => none
  -- A byte below 255 is the length; 255 means four bytes follow.
  | b :: rest => if b < long then some (b, rest) else readBe32 rest

-- Promise: any length a `u32` can hold reads back as itself, and the bytes
-- after it are left untouched.
theorem readLen_lenBytes (n : Nat) (rest : List Nat) (h : n < 4294967296) :
    readLen (lenBytes n ++ rest) = some (n, rest) := by
  -- Split on whether the length is short.
  unfold lenBytes
  by_cases hn : n < long
  -- Short: the one byte is read as the length.
  · simp [hn, readLen]
  -- Long: the flag is skipped and the four bytes read as the length.
  · rw [if_neg hn]
    show readLen (long :: (be32 n ++ rest)) = some (n, rest)
    simp only [readLen, long, Nat.lt_irrefl, if_false]
    exact readBe32_be32 n rest h

-- A length always takes at least one byte.
theorem lenBytes_cons (n : Nat) : ∃ b bs, lenBytes n = b :: bs := by
  -- Both forms start with a byte.
  unfold lenBytes
  split
  · exact ⟨_, _, rfl⟩
  · exact ⟨_, _, rfl⟩

-- A key belongs to the document in the index: it is the prefix, some values,
-- then the suffix.
def Framed (p s k : List Nat) : Prop := ∃ m, k = p ++ m ++ s

-- The values of a key: drop the prefix, keep what comes before the suffix
-- (`Frame::values`).
def mid (p s k : List Nat) : List Nat :=
  (k.drop p.length).take (k.length - p.length - s.length)

-- Promise: the values cut out of a framed key are exactly its values.
theorem mid_frame (p m s : List Nat) : mid p s (p ++ m ++ s) = m := by
  -- Dropping the prefix leaves the values then the suffix; the count left
  -- once the prefix and suffix are taken off is the values' length.
  have hdrop : (p ++ m ++ s).drop p.length = m ++ s := by
    simp [List.append_assoc]
  simp only [mid, hdrop, List.length_append]
  have hcount : p.length + m.length + s.length - p.length - s.length = m.length := by
    omega
  rw [hcount]
  simp

-- Each key's values behind their length, one key after another.
def body (p s : List Nat) : List (List Nat) → List Nat
  -- No keys: no bytes.
  | [] => []
  -- A key's values, then the rest.
  | k :: ks => lenBytes (mid p s k).length ++ mid p s k ++ body p s ks

-- A record of values: the tag, then the body (`encode_record`).
def encodeValues (p s : List Nat) (ks : List (List Nat)) : List Nat :=
  valuesTag :: body p s ks

-- Read the body until its bytes run out, putting the prefix and suffix back
-- around each value. `fuel` bounds the keys read, so the reading always ends.
def decodeBody (p s : List Nat) : Nat → List Nat → Option (List (List Nat))
  -- No bytes left: every key has been read.
  | _, [] => some []
  -- Bytes left but no fuel: more keys than the bound allows.
  | 0, _ :: _ => none
  -- Read a length, take that many bytes as the values, rebuild the key.
  | fuel + 1, b :: bs =>
    match readLen (b :: bs) with
    | none => none
    | some (len, rest) =>
      if len ≤ rest.length then
        (decodeBody p s fuel (rest.drop len)).map ((p ++ rest.take len ++ s) :: ·)
      else none

-- Read either form (`decode_record`): a record starting with the tag keeps
-- values; any other is a record of whole keys.
def decodeRecord (p s : List Nat) : List Nat → Option (List (List Nat))
  -- An empty record names no entries.
  | [] => some []
  -- The first byte says which form it is.
  | b :: rest =>
    if b = valuesTag then decodeBody p s rest.length rest
    else decode (b :: rest).length (b :: rest)

-- Promise: with fuel for every key, the body reads back as exactly its keys.
theorem decodeBody_body (p s : List Nat) (ks : List (List Nat))
    -- Given every key belongs to the document in the index,
    (hframed : ∀ k ∈ ks, Framed p s k)
    -- and every key's values fit a `u32` length,
    (hfit : ∀ k ∈ ks, (mid p s k).length < 4294967296) :
    -- any fuel at least the number of keys reads them all back.
    ∀ fuel, ks.length ≤ fuel → decodeBody p s fuel (body p s ks) = some ks := by
  -- Induct on the keys.
  induction ks with
  -- No keys: no bytes, read as no keys.
  | nil => intro fuel _; cases fuel <;> rfl
  -- A key then the rest: the key reads back, then the rest do.
  | cons k ks ih =>
    intro fuel hfuel
    -- The fuel covers this key, so it is one more than some smaller fuel.
    obtain ⟨f, rfl⟩ : ∃ f, fuel = f + 1 := ⟨fuel - 1, by simp at hfuel; omega⟩
    -- The body starts with a length byte, so the reading takes the third case.
    obtain ⟨b, bs, hlen⟩ := lenBytes_cons (mid p s k).length
    have hbytes : body p s (k :: ks) = b :: (bs ++ mid p s k ++ body p s ks) := by
      simp [body, hlen, List.append_assoc]
    rw [hbytes]
    simp only [decodeBody]
    rw [← hbytes]
    -- Those bytes are the values' length, the values, then the rest.
    have hread : readLen (body p s (k :: ks)) =
        some ((mid p s k).length, mid p s k ++ body p s ks) := by
      simp only [body, List.append_assoc]
      exact readLen_lenBytes _ _ (hfit k (by simp))
    rw [hread]
    -- The values are all there, so they are taken and the rest read on.
    simp only [List.length_append, Nat.le_add_right, if_true, List.take_left',
      List.drop_left']
    have hrestf : ∀ q ∈ ks, Framed p s q := fun q hq => hframed q (by simp [hq])
    have hrestfit : ∀ q ∈ ks, (mid p s q).length < 4294967296 :=
      fun q hq => hfit q (by simp [hq])
    rw [ih hrestf hrestfit f (by simp at hfuel; omega)]
    -- Putting the prefix and suffix back around the values gives the key.
    obtain ⟨m, rfl⟩ := hframed k (by simp)
    simp only [mid_frame]
    rfl

-- Each key adds at least one byte to the body.
theorem length_le_body (p s : List Nat) (ks : List (List Nat)) :
    ks.length ≤ (body p s ks).length := by
  -- Count key by key: a length takes a byte or more.
  induction ks with
  | nil => simp [body]
  | cons k ks ih =>
    obtain ⟨b, bs, hlen⟩ := lenBytes_cons (mid p s k).length
    simp only [body, hlen, List.length_append, List.length_cons]
    omega

-- Promise: a record of values reads back as exactly its keys, in order.
theorem decodeRecord_encodeValues (p s : List Nat) (ks : List (List Nat))
    -- Given every key belongs to the document in the index,
    (hframed : ∀ k ∈ ks, Framed p s k)
    -- and every key's values fit a `u32` length,
    (hfit : ∀ k ∈ ks, (mid p s k).length < 4294967296) :
    decodeRecord p s (encodeValues p s ks) = some ks := by
  -- The tag sends the reading to the body, with its length as fuel.
  simp only [encodeValues, decodeRecord, if_true]
  exact decodeBody_body p s ks hframed hfit _ (length_le_body p s ks)

-- Promise: a record of whole keys, each at most regolith's 8 MiB limit, starts
-- with the byte 0 unless it is empty, so it is never taken for values.
theorem encode_starts_with_zero (k : List Nat) (ks : List (List Nat))
    (hk : k.length ≤ 8388608) :
    ∃ rest, encode (k :: ks) = 0 :: rest := by
  -- The first byte is the top byte of the first key's length, which is 0.
  refine ⟨[k.length / 65536 % 256, k.length / 256 % 256, k.length % 256] ++ k ++ encode ks, ?_⟩
  have htop : k.length / 16777216 % 256 = 0 := by
    rw [Nat.div_eq_of_lt (by omega)]
  simp only [encode, be32, htop, List.cons_append, List.nil_append, List.append_assoc]

-- Promise: a record of whole keys, each at most regolith's 8 MiB limit, still
-- reads back as exactly its keys.
theorem decodeRecord_encode (p s : List Nat) (ks : List (List Nat))
    -- Given every key is at most 8 MiB, as the store's open enforces,
    (hsmall : ∀ k ∈ ks, k.length ≤ 8388608) :
    decodeRecord p s (encode ks) = some ks := by
  cases ks with
  -- No keys: an empty record, read as no keys.
  | nil => rfl
  -- Keys: the record starts with 0, not the tag, so it is read as whole keys.
  | cons k ks =>
    obtain ⟨rest, hzero⟩ := encode_starts_with_zero k ks (hsmall k (by simp))
    have hfit : ∀ q ∈ k :: ks, q.length < 4294967296 :=
      fun q hq => by
        have hq := hsmall q hq
        omega
    rw [hzero]
    simp only [decodeRecord, valuesTag, Nat.zero_ne_one, if_false]
    rw [← hzero]
    exact decode_encode (k :: ks) hfit

end PositionRecord
