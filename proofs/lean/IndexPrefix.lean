/-!
The story: truncating a collection deletes its secondary index entries by key
prefix. Every key in the datastore is a list of bytes. An index entry's key
starts with `/`, the collection's short ID, `/`, the index's ID, `/`, and both
IDs are written with `encode_uvarint_ascending`: an ID below 240 is one raw
byte, a larger one a marker byte of 240 or more followed by more bytes.
Documents, deletion markers, schema versions, searchable-encryption tags and
view caches live in families whose keys start `/tag/name/`, where `tag` is
plain text (`d`, `del`, `v`, `se`, `collection`) and `name` is a collection ID
or another text of two or more characters with no `/` in it.

What used to go wrong: the truncate deleted the collection's whole index
prefix, `/` + short ID + `/`. For short ID 100 that is `/d/`, the start of
every collection's document keys, so truncating one collection wiped every
collection's documents (118 is `/v/`, the schema versions).

What must hold now that the truncate deletes one index at a time:
  - the prefix of an index covers every entry of that index;
  - it covers no entry of any other index, of this collection or another;
  - it covers no key of a `/tag/name/` family, for any short ID.

The IDs are `u32` in the code (`IndexDataStoreKey::index_prefix` takes
`collection_short_id: u32` and `index_id: u32`), so the long form here needs at
most four bytes after its marker.

Rust owners:
  crates/engine/storage/src/keys/utils/mod.rs encode_uvarint_ascending
  crates/engine/storage/src/keys/datastore/index_key.rs IndexDataStoreKey::index_prefix
  crates/engine/storage/src/keys/document.rs doc_key, deleted_doc_key
  crates/database/db/src/collection/ops/truncate/all.rs DB::truncate_metadata_in
-/

namespace IndexPrefix

-- The separator byte, `/`.
def sep : Nat := 47

-- A value fits in a `u32`.
def U32 (v : Nat) : Prop := v < 4294967296

-- `n` bytes of `v`, most significant first.
def beBytes : Nat → Nat → List Nat
  | 0, _ => []
  | n + 1, v => beBytes n (v / 256) ++ [v % 256]

-- How many bytes a long value (2288 up to the `u32` limit) takes: the Rust
-- `bytes_needed`, the length of its significant bits rounded up to bytes.
def byteCount (v : Nat) : Nat :=
  if v < 65536 then 2 else if v < 16777216 then 3 else 4

-- `encode_uvarint_ascending`: one raw byte below 240; two bytes from 0xF0 up
-- to 2287; otherwise the marker 0xF7 + byte count, then the bytes.
def enc (v : Nat) : List Nat :=
  if v < 240 then [v]
  else if v ≤ 2287 then [240 + (v - 240) / 256, (v - 240) % 256]
  else (247 + byteCount v) :: beBytes (byteCount v) v

-- `IndexDataStoreKey::index_prefix`: `/` short ID `/` index ID `/`.
def indexPrefix (s i : Nat) : List Nat := [sep] ++ enc s ++ [sep] ++ enc i ++ [sep]

-- The collection-wide prefix the truncate used to delete: `/` short ID `/`.
def collectionPrefix (s : Nat) : List Nat := [sep] ++ enc s ++ [sep]

-- A key of a `/tag/name/` family, with whatever follows the name.
def familyKey (tag name rest : List Nat) : List Nat :=
  [sep] ++ tag ++ [sep] ++ name ++ [sep] ++ rest

-- Deleting the prefix `p` takes the key `k`: `k` starts with `p`.
def Covers (p k : List Nat) : Prop := ∃ t, p ++ t = k

-- Plain text: every byte is ASCII and none is the separator.
def Text (l : List Nat) : Prop := ∀ b ∈ l, b < 128 ∧ b ≠ sep

-- The value a list of big-endian bytes spells.
def beValue (l : List Nat) : Nat := l.foldl (fun acc b => acc * 256 + b) 0

-- Read an encoded value back: the inverse of `enc` on `u32` values.
def dec : List Nat → Nat
  | [] => 0
  | h :: t =>
    if h < 240 then h
    else if h ≤ 247 then 240 + (h - 240) * 256 + t.headD 0
    else beValue t

-- How many bytes an encoding takes, read from its first byte alone.
def width (h : Nat) : Nat :=
  if h < 240 then 1 else if h ≤ 247 then 2 else h - 246

---------------------------------------------------------------- the old bug

-- The old collection-wide prefix of short ID 100 covers every document key:
-- `/d/` is the separator, the byte 100, the separator.
theorem old_prefix_of_100_covers_documents (name rest : List Nat) :
    Covers (collectionPrefix 100) (familyKey [100] name rest) := by
  -- What follows `/d/` in the document key is what the prefix leaves.
  exact ⟨name ++ [sep] ++ rest, by simp [collectionPrefix, familyKey, enc]⟩

-- Short ID 118 likewise covers every schema-version key, under `/v/`.
theorem old_prefix_of_118_covers_versions (name rest : List Nat) :
    Covers (collectionPrefix 118) (familyKey [118] name rest) := by
  -- What follows `/v/` in the version key is what the prefix leaves.
  exact ⟨name ++ [sep] ++ rest, by simp [collectionPrefix, familyKey, enc]⟩

---------------------------------------------------------------- the encoding

-- A value below 240 is the one byte it is.
theorem enc_small {v : Nat} (h : v < 240) : enc v = [v] := by
  -- The first branch of `enc` applies.
  simp [enc, h]

-- A value of 240 or more starts with a byte of 240 or more.
theorem enc_large {v : Nat} (h : 240 ≤ v) : ∃ hd tl, enc v = hd :: tl ∧ 240 ≤ hd := by
  -- Not the one-byte branch.
  have hn : ¬ v < 240 := by omega
  unfold enc
  rw [if_neg hn]
  -- Both remaining branches start with a marker of 240 or more.
  split
  · exact ⟨_, _, rfl, by omega⟩
  · exact ⟨_, _, rfl, by omega⟩

-- `n` big-endian bytes are `n` bytes long.
theorem beBytes_length : ∀ n v, (beBytes n v).length = n
  | 0, _ => rfl
  | n + 1, v => by
    -- One byte more than the bytes of `v / 256`.
    simp [beBytes, beBytes_length n (v / 256)]

-- Appending a byte multiplies the value so far by 256 and adds it.
theorem beValue_snoc (l : List Nat) (b : Nat) : beValue (l ++ [b]) = beValue l * 256 + b := by
  -- A left fold over an append folds the last byte last.
  simp [beValue, List.foldl_append]

-- Reading back `n` bytes of a value below `256 ^ n` gives the value.
theorem beValue_beBytes : ∀ n v, v < 256 ^ n → beValue (beBytes n v) = v
  | 0, v, h => by
    -- No bytes: the value must be zero.
    simp [beBytes, beValue] at *
    omega
  | n + 1, v, h => by
    -- The leading bytes hold `v / 256`, which fits in `n` bytes.
    have hq : v / 256 < 256 ^ n := by
      rw [Nat.pow_succ] at h
      exact (Nat.div_lt_iff_lt_mul (by decide)).2 h
    rw [beBytes, beValue_snoc, beValue_beBytes n (v / 256) hq]
    -- And `v / 256 * 256 + v % 256` is `v`.
    omega

-- A long `u32` value fits in the bytes `byteCount` gives it.
theorem lt_byteCount {v : Nat} (hv : U32 v) : v < 256 ^ byteCount v := by
  -- Each branch of `byteCount` is chosen by exactly this bound.
  unfold byteCount U32 at *
  split
  · simp; omega
  · split
    · simp; omega
    · simp; omega

-- Decoding an encoded `u32` value gives the value back.
theorem dec_enc {v : Nat} (hv : U32 v) : dec (enc v) = v := by
  by_cases h1 : v < 240
  · -- One byte: it is the value.
    simp [enc, dec, h1]
  · by_cases h2 : v ≤ 2287
    · -- Two bytes: the marker carries the high part, the next byte the rest.
      have hm : ¬ 240 + (v - 240) / 256 < 240 := by omega
      have hl : 240 + (v - 240) / 256 ≤ 247 := by omega
      simp only [enc, if_neg h1, if_pos h2, dec, if_neg hm, if_pos hl, List.headD]
      omega
    · -- The long form: a marker above 247, then the big-endian bytes.
      have hc : 2 ≤ byteCount v := by unfold byteCount; split <;> (try split) <;> omega
      have hm : ¬ 247 + byteCount v < 240 := by omega
      have hl : ¬ 247 + byteCount v ≤ 247 := by omega
      simp only [enc, if_neg h1, if_neg h2, dec, if_neg hm, if_neg hl]
      exact beValue_beBytes _ _ (lt_byteCount hv)

-- Two `u32` values with the same encoding are equal.
theorem enc_inj {a b : Nat} (ha : U32 a) (hb : U32 b) (h : enc a = enc b) : a = b := by
  -- Decode both sides.
  rw [← dec_enc ha, ← dec_enc hb, h]

-- An encoding is as long as its first byte says.
theorem enc_width (v : Nat) : ∃ hd tl, enc v = hd :: tl ∧ (hd :: tl).length = width hd := by
  by_cases h1 : v < 240
  · -- One byte below 240.
    exact ⟨v, [], enc_small h1, by simp [width, h1]⟩
  · by_cases h2 : v ≤ 2287
    · -- Two bytes, marker 240 to 247.
      refine ⟨240 + (v - 240) / 256, [(v - 240) % 256], by simp [enc, h1, h2], ?_⟩
      have hm : ¬ 240 + (v - 240) / 256 < 240 := by omega
      have hl : 240 + (v - 240) / 256 ≤ 247 := by omega
      simp [width, hm, hl]
    · -- The marker 247 + n, then n bytes.
      refine ⟨247 + byteCount v, beBytes (byteCount v) v, by simp [enc, h1, h2], ?_⟩
      have hc : 2 ≤ byteCount v := by unfold byteCount; split <;> (try split) <;> omega
      have hm : ¬ 247 + byteCount v < 240 := by omega
      have hl : ¬ 247 + byteCount v ≤ 247 := by omega
      simp [width, hm, hl, beBytes_length]
      omega

-- When two keys agree and each starts with an encoded `u32`, the two values
-- are equal and so is what follows them: the encoding is prefix-free.
theorem enc_split {a b : Nat} {x y : List Nat} (ha : U32 a) (hb : U32 b)
    (h : enc a ++ x = enc b ++ y) : a = b ∧ x = y := by
  obtain ⟨ha0, ta, hea, hwa⟩ := enc_width a
  obtain ⟨hb0, tb, heb, hwb⟩ := enc_width b
  -- The first bytes agree, so the encodings are the same length.
  have hfirst : ha0 = hb0 := by
    rw [hea, heb] at h
    exact (List.cons.inj h).1
  have hlen : (enc a).length = (enc b).length := by
    rw [hea, heb, hwa, hwb, hfirst]
  -- Same length, same keys: the encodings are equal, and the rests too.
  obtain ⟨he, hr⟩ := List.append_inj h hlen
  exact ⟨enc_inj ha hb he, hr⟩

---------------------------------------------------------------- the per-index prefix

-- An index's prefix covers every entry of that index.
theorem index_prefix_takes_its_entries (s i : Nat) (fields : List Nat) :
    Covers (indexPrefix s i) (indexPrefix s i ++ fields) :=
  -- The entry is the prefix followed by its fields.
  ⟨fields, rfl⟩

-- An index's prefix covers an entry of another index only if both IDs match:
-- it never deletes an entry of another index of this collection or another.
theorem index_prefix_only_its_index {s i s' i' : Nat} (hs : U32 s) (hi : U32 i)
    (hs' : U32 s') (hi' : U32 i') (fields : List Nat)
    (h : Covers (indexPrefix s i) (indexPrefix s' i' ++ fields)) : s = s' ∧ i = i' := by
  obtain ⟨t, h⟩ := h
  -- Line both keys up after their first separator.
  simp only [indexPrefix, List.append_assoc, List.singleton_append, List.cons_append,
    List.cons.injEq, true_and] at h
  -- The short IDs come first and match.
  obtain ⟨hsid, h⟩ := enc_split hs hs' h
  -- Then a separator, then the index IDs, which match too.
  simp only [List.cons.injEq, true_and] at h
  exact ⟨hsid, (enc_split hi hi' h).1⟩

-- An index's prefix covers no key of a `/tag/name/` family, whatever the
-- short ID: not the documents (`/d/`), deletion markers (`/del/`), schema
-- versions (`/v/`), encryption tags (`/se/`) or view caches
-- (`/collection/vi/`).
theorem index_prefix_misses_family (s i : Nat) {tag name : List Nat} (rest : List Nat)
    (htag : Text tag) (hne : tag ≠ []) (hname : Text name) (hlen : 2 ≤ name.length) :
    ¬ Covers (indexPrefix s i) (familyKey tag name rest) := by
  intro ⟨t, h⟩
  -- Both keys start with the separator; compare what follows it.
  have h0 : enc s ++ sep :: (enc i ++ sep :: t) = tag ++ sep :: (name ++ sep :: rest) := by
    have := (List.cons.inj h).2
    simpa [List.append_assoc] using this
  cases tag with
  | nil => exact hne rfl
  | cons t0 tr =>
    by_cases hsm : s < 240
    · -- A one-byte short ID meets the tag's first byte; the separator after
      -- it meets the tag's second byte, unless the tag is one byte long.
      rw [enc_small hsm] at h0
      have h1 := (List.cons.inj h0).2
      cases tr with
      | cons t1 tr' =>
        -- The tag's second byte would be the separator.
        exact (htag t1 (by simp)).2 (List.cons.inj h1).1.symm
      | nil =>
        -- A one-byte tag: the index ID meets the name.
        have h2 := (List.cons.inj h1).2
        match name, hname, hlen, h2 with
        | [], _, hlen, _ => simp at hlen
        | [_], _, hlen, _ => simp at hlen
        | n0 :: n1 :: nr, hname, _, h2 =>
          by_cases him : i < 240
          · -- A one-byte index ID: the separator after it would be the
            -- name's second byte.
            rw [enc_small him] at h2
            have h3 := (List.cons.inj h2).2
            exact (hname n1 (by simp)).2 (List.cons.inj h3).1.symm
          · -- A longer index ID starts at 240 or more, above any text byte.
            obtain ⟨hd, tl, he, hge⟩ := enc_large (Nat.le_of_not_lt him)
            rw [he] at h2
            have h3 := (List.cons.inj h2).1
            have := (hname n0 (by simp)).1
            omega
    · -- A longer short ID starts at 240 or more, above any text byte.
      obtain ⟨hd, tl, he, hge⟩ := enc_large (Nat.le_of_not_lt hsm)
      rw [he] at h0
      have h3 := (List.cons.inj h0).1
      have := (htag t0 (by simp)).1
      omega

end IndexPrefix
