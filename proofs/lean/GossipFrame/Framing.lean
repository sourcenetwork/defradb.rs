/-!
# GossipFrame.Framing: gossip frames on the mux stream come back exactly as sent

The story, in plain words. Gossip between two nodes runs on one long-lived
stream of the connection they already share. The stream opens with a tag that
says "this is gossip", and then carries messages one after another. Each
message is a frame: four bytes giving its length, then that many bytes. A
reader takes the frames off the stream in order and refuses any frame longer
than the limit, before reading it.

What must never happen:
* a frame comes back different from the one sent, or a later frame is
  misread (`decode_encode`, `decode_all_encode_all`);
* a frame over the limit is accepted (`decode_rejects_oversized`,
  `decoded_within_limit`);
* a tag comes back different from the one written, or a tag longer than
  the tag limit is accepted (`read_tag_write_tag`, `read_tag_within_limit`).

Bytes are modelled as natural numbers below 256. The message inside a frame is
iroh-gossip's own encoding and is not covered here.

Rust anchors: `write_gossip_frame`, `read_gossip_frame` in
`crates/p2p/src/iroh/gossip/frame.rs`; `write_stream_tag`, `read_stream_tag`
in `crates/p2p/src/iroh/protocols.rs`.
-/

namespace GossipFrame

-- A length as four big-endian bytes, as `u32::to_be_bytes` writes it.
def be32 (n : Nat) : List Nat :=
  [n / 16777216 % 256, n / 65536 % 256, n / 256 % 256, n % 256]

-- Read four big-endian bytes as a length; nothing if fewer than four remain.
def readBe32 : List Nat → Option (Nat × List Nat)
  -- Four bytes or more: combine the first four, keep the rest;
  | a :: b :: c :: d :: rest => some (a * 16777216 + b * 65536 + c * 256 + d, rest)
  -- fewer: the length is not all here yet.
  | _ => none

-- Promise: any length a `u32` can hold reads back as itself, and the bytes
-- after it are left untouched.
theorem readBe32_be32 (n : Nat) (rest : List Nat) (h : n < 4294967296) :
    readBe32 (be32 n ++ rest) = some (n, rest) := by
  -- Unfold both sides; what is left is arithmetic on the four digits.
  simp only [be32, readBe32, List.cons_append, List.nil_append, Option.some.injEq,
    Prod.mk.injEq, and_true]
  omega

-- A frame: the payload's length, then the payload.
def encodeFrame (payload : List Nat) : List Nat := be32 payload.length ++ payload

-- Read one frame: its length, refused if over `limit`, then that many bytes.
-- Nothing if the length is over the limit or the bytes are not all here.
def decodeFrame (limit : Nat) (bytes : List Nat) : Option (List Nat × List Nat) :=
  match readBe32 bytes with
  -- No full length yet: no frame.
  | none => none
  -- A length: accept it only within the limit and with all its bytes present.
  | some (len, rest) =>
    if len ≤ limit ∧ len ≤ rest.length then some (rest.take len, rest.drop len) else none

-- Promise: a frame within the limit reads back as itself, and whatever
-- follows it is left for the next read.
theorem decode_encode (limit : Nat) (payload rest : List Nat)
    -- Given the payload fits the limit,
    (hfit : payload.length ≤ limit)
    -- and the limit fits a `u32`,
    (hlimit : limit < 4294967296) :
    -- the frame reads back exactly, with the rest untouched.
    decodeFrame limit (encodeFrame payload ++ rest) = some (payload, rest) := by
  -- The length prefix reads back first.
  have hlen := readBe32_be32 payload.length (payload ++ rest) (by omega)
  -- Then the payload is taken and the rest dropped.
  simp only [decodeFrame, encodeFrame, List.append_assoc, hlen, List.length_append]
  simp [hfit]

-- Promise: a frame that announces more than the limit is refused, whatever
-- bytes follow.
theorem decode_rejects_oversized (limit len : Nat) (bytes rest : List Nat)
    -- Given the stream announces a length `len`,
    (hread : readBe32 bytes = some (len, rest))
    -- and that length is over the limit,
    (hover : limit < len) :
    -- no frame is read.
    decodeFrame limit bytes = none := by
  -- The limit check fails before any payload is taken.
  simp only [decodeFrame, hread]
  split
  -- The check passing would need `len ≤ limit`, which contradicts `hover`.
  · omega
  -- The check failing gives no frame, as promised.
  · rfl

-- Promise: every frame read is within the limit.
theorem decoded_within_limit (limit : Nat) (bytes payload rest : List Nat)
    -- Given a frame was read,
    (h : decodeFrame limit bytes = some (payload, rest)) :
    -- its payload is no longer than the limit.
    payload.length ≤ limit := by
  -- Look at the length the stream announced.
  unfold decodeFrame at h
  split at h
  -- No length: no frame was read, which contradicts `h`.
  · cases h
  -- A length: the frame came from taking `len` bytes after the check passed.
  · rename_i len tail _
    split at h
    · rename_i hok
      -- The payload is the first `len` bytes, so it is at most `len` long.
      cases h
      simp only [List.length_take]
      omega
    · cases h

-- Write frames one after another on the stream.
def encodeAll (payloads : List (List Nat)) : List Nat :=
  (payloads.map encodeFrame).foldr (· ++ ·) []

-- Read `count` frames off the stream, in order; nothing if any is refused.
def decodeAll (limit : Nat) : Nat → List Nat → Option (List (List Nat))
  -- No more frames wanted: done.
  | 0, _ => some []
  -- One more: read a frame, then the rest from what follows it.
  | count + 1, bytes =>
    match decodeFrame limit bytes with
    | none => none
    | some (payload, rest) => (decodeAll limit count rest).map (payload :: ·)

-- Promise: frames written one after another read back in the same order,
-- each one exactly as written.
theorem decode_all_encode_all (limit : Nat) (payloads : List (List Nat))
    -- Given every payload fits the limit,
    (hfit : ∀ p ∈ payloads, p.length ≤ limit)
    -- and the limit fits a `u32`,
    (hlimit : limit < 4294967296) :
    -- reading as many frames as were written returns them all, in order.
    decodeAll limit payloads.length (encodeAll payloads) = some payloads := by
  -- Induct on the frames written.
  induction payloads with
  -- No frames: nothing to read.
  | nil => rfl
  -- One frame then the rest: the first reads back, then the rest do.
  | cons p rest ih =>
    -- The first frame is within the limit.
    have hp := hfit p (by simp)
    -- So are the rest.
    have hrest : ∀ q ∈ rest, q.length ≤ limit := fun q hq => hfit q (by simp [hq])
    -- Read the first frame, then use the rest's promise.
    simp only [encodeAll, List.map_cons, List.foldr_cons, List.length_cons, decodeAll]
    rw [decode_encode limit p _ hp hlimit]
    simp only [encodeAll] at ih
    simp [ih hrest]

-- The longest stream tag, `MAX_STREAM_TAG_LEN`.
def maxTagLen : Nat := 64

-- A tag on the wire: one byte of length, then the tag.
def writeTag (tag : List Nat) : List Nat := tag.length :: tag

-- Read a tag: refused if its length byte is over the tag limit or the tag is
-- not all here.
def readTag : List Nat → Option (List Nat × List Nat)
  -- A length byte: accept it only within the limit and with the whole tag present;
  | len :: rest =>
    if len ≤ maxTagLen ∧ len ≤ rest.length then some (rest.take len, rest.drop len) else none
  -- nothing at all: no tag.
  | [] => none

-- Promise: a tag within the limit reads back as itself, with the stream after
-- it untouched.
theorem read_tag_write_tag (tag rest : List Nat)
    -- Given the tag is no longer than the tag limit,
    (h : tag.length ≤ maxTagLen) :
    -- it reads back exactly.
    readTag (writeTag tag ++ rest) = some (tag, rest) := by
  -- The length byte passes the check, then the tag is taken and the rest kept.
  simp [readTag, writeTag, h]

-- Promise: every tag read is within the tag limit.
theorem read_tag_within_limit (bytes tag rest : List Nat)
    -- Given a tag was read,
    (h : readTag bytes = some (tag, rest)) :
    -- it is no longer than the tag limit.
    tag.length ≤ maxTagLen := by
  -- Look at the stream's first byte.
  unfold readTag at h
  split at h
  -- A length byte: the tag is the first `len` bytes after the check passed.
  · split at h
    · rename_i len tail hok
      cases h
      simp only [List.length_take]
      omega
    · cases h
  -- An empty stream gives no tag, which contradicts `h`.
  · cases h

end GossipFrame
