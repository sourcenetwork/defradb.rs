/-!
A policy nonce as text, and back.

vera.rs names a policy by its definition, its creator and a 32-byte nonce the
creator picks. DefraDB takes a caller's nonce as 64 hexadecimal characters
(HTTP, the CLI, a host's configuration) and writes one the same way. Each
byte becomes two digits, the high half first.

What must never happen: a nonce written out reads back as another nonce, two
nonces are written the same, a text of the wrong length is taken as a nonce,
or a nonce is written with a character that is not a hexadecimal digit.

Rust owner: `PolicyNonce`'s `Display` and `FromStr` in
`crates/engine/acp/src/policy_nonce.rs`.

The model is over digit values (0 to 15), not characters: that a digit's
character reads back as its value, in either case, is the character table's,
assumed here.
-/
namespace VeraPolicyNonce

/-- The bytes in a nonce. -/
def nonceBytes : Nat := 32

/-- The digits of a byte: its high half, then its low half. -/
def digits (byte : Nat) : List Nat :=
  -- Dividing by 16 gives the high half, the remainder the low half.
  [byte / 16, byte % 16]

/-- A nonce written out: every byte's two digits, in order. -/
def encode (bytes : List Nat) : List Nat :=
  -- One byte after the other, each as its two digits.
  bytes.flatMap digits

/-- Digits read back two at a time; `none` for an odd count or a value that
is not a digit. -/
def decode : List Nat → Option (List Nat)
  -- No digits are no bytes.
  | [] => some []
  -- One digit left over is half a byte: refused.
  | [_] => none
  -- Two digits make one byte, if both are digits; the rest follows.
  | high :: low :: rest =>
      if high < 16 ∧ low < 16 then
        (decode rest).map (fun bytes => (16 * high + low) :: bytes)
      else none

/-- A text taken as a nonce: exactly 64 digits, read back. -/
def parse (text : List Nat) : Option (List Nat) :=
  -- Any other length is refused before a digit is read.
  if text.length = 2 * nonceBytes then decode text else none

/-- A written byte reads back as itself. -/
theorem byte_round_trip (byte : Nat) :
    16 * (byte / 16) + byte % 16 = byte := by
  -- Division with remainder puts the byte back together.
  omega

/-- Writing out bytes and reading the digits back gives the same bytes. -/
theorem decode_encode (bytes : List Nat) (h : ∀ b ∈ bytes, b < 256) :
    decode (encode bytes) = some bytes := by
  -- One byte at a time, from the first.
  induction bytes with
  | nil =>
    -- No bytes are written as no digits, which read back as no bytes.
    rfl
  | cons byte rest ih =>
    -- The first byte's two digits are digits, and read back as the byte.
    have hb : byte < 256 := h byte (List.mem_cons_self byte rest)
    -- The rest of the bytes are bytes too.
    have hr : ∀ b ∈ rest, b < 256 := fun b hm => h b (List.mem_cons_of_mem byte hm)
    -- Both halves are below 16.
    have hhigh : byte / 16 < 16 := by omega
    have hlow : byte % 16 < 16 := Nat.mod_lt _ (by decide)
    -- Unfold one byte, read it back, and use the rest's round trip.
    simp only [encode, List.flatMap_cons, digits, List.cons_append, List.nil_append]
    simp only [decode, hhigh, hlow, and_self, if_true]
    rw [show List.flatMap digits rest = encode rest from rfl, ih hr]
    simp [byte_round_trip]

/-- Every byte is written as exactly two digits. -/
theorem encode_length (bytes : List Nat) :
    (encode bytes).length = 2 * bytes.length := by
  -- One byte at a time: two digits each.
  induction bytes with
  | nil => rfl
  | cons byte rest ih =>
    simp only [encode, List.flatMap_cons, digits, List.length_append, List.length_cons,
      List.length_nil] at ih ⊢
    -- Two digits for this byte, two for every byte after it.
    have : (encode rest).length = 2 * rest.length := ih
    omega

/-- A nonce is written only with hexadecimal digits. -/
theorem encode_digits (bytes : List Nat) (h : ∀ b ∈ bytes, b < 256) :
    ∀ d ∈ encode bytes, d < 16 := by
  -- Any written digit is one half of some byte of the nonce.
  intro d hd
  simp only [encode, List.mem_flatMap, digits, List.mem_cons, List.mem_nil_iff,
    or_false] at hd
  obtain ⟨byte, hmem, hd⟩ := hd
  -- That byte is below 256.
  have hb := h byte hmem
  -- Its high half is below 256 / 16 = 16, its low half below 16.
  rcases hd with rfl | rfl <;> omega

/-- A nonce written out is taken back as the same nonce. -/
theorem parse_encode (bytes : List Nat) (hlen : bytes.length = nonceBytes)
    (h : ∀ b ∈ bytes, b < 256) : parse (encode bytes) = some bytes := by
  -- Its 32 bytes are written as 64 digits, which pass the length check and
  -- read back as the bytes.
  simp only [parse, encode_length, hlen, if_true, decode_encode bytes h]

/-- Two nonces written the same are the same nonce. -/
theorem encode_injective (a b : List Nat) (ha : ∀ x ∈ a, x < 256)
    (hb : ∀ x ∈ b, x < 256) (h : encode a = encode b) : a = b := by
  -- Each reads back as itself, and both read back from the same digits.
  have := decode_encode a ha
  rw [h, decode_encode b hb] at this
  exact (Option.some.inj this).symm

/-- Digits read back are half as many bytes. -/
theorem decode_length : ∀ (text bytes : List Nat), decode text = some bytes →
    text.length = 2 * bytes.length
  -- No digits read back as no bytes.
  | [], bytes, h => by
    simp only [decode, Option.some.injEq] at h
    subst h
    rfl
  -- One digit is refused, so it never reads back.
  | [_], _, h => by simp [decode] at h
  -- Two digits read back as one byte, and the rest as the rest.
  | high :: low :: rest, bytes, h => by
    simp only [decode] at h
    split at h
    · obtain ⟨tail, htail, rfl⟩ := Option.map_eq_some'.mp h
      have := decode_length rest tail htail
      simp only [List.length_cons]
      omega
    · simp at h

/-- A text taken as a nonce had 64 digits and gives 32 bytes. -/
theorem parse_length (text bytes : List Nat) (h : parse text = some bytes) :
    text.length = 2 * nonceBytes ∧ bytes.length = nonceBytes := by
  -- Only a 64-digit text gets past the length check.
  unfold parse at h
  split at h
  · -- And 64 digits read back as 32 bytes.
    have := decode_length text bytes h
    exact ⟨by assumption, by simp only [nonceBytes] at *; omega⟩
  · simp at h

end VeraPolicyNonce
