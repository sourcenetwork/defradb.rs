/-!
# GossipFrame.Budget: every message gossip sends fits one of our frames

The story, in plain words. iroh-gossip sends a broadcast only if it fits its
budget of `max_message_size - 2` bytes, and it keeps every other message it
sends under `max_message_size` too. Its own wire sends a topic once per
stream, so that budget leaves the topic out. Our frame carries the topic in
every message: 32 more bytes. The frame limit is therefore
`max_message_size + 32`, and the largest content gossip carries is 4,055
bytes. A publish larger than that is refused before it reaches gossip.

What must never happen:
* gossip sends a message that is over our frame limit, so the writer cannot
  send it (`broadcast_fits_frame`, `emitted_fits_frame`);
* the content budget we check is not gossip's own: it lets through content
  gossip would drop, or refuses content gossip carries
  (`max_broadcast_fits`, `over_max_broadcast_dropped`).

Sizes are bytes, as natural numbers. The encoding of a broadcast follows
iroh-gossip 0.102's `plumtree::max_gossip_size`; the Rust test
`the_broadcast_budget_is_the_protocols_own` checks it against the real
protocol.

Rust anchors: `MAX_MESSAGE_SIZE`, `TOPIC_ID_LEN`, `FRAME_LIMIT`,
`BROADCAST_OVERHEAD`, `MAX_BROADCAST`, `varint_len`, `max_broadcast` in
`crates/p2p/src/iroh/gossip/frame.rs`.
-/

namespace GossipFrame

-- iroh-gossip's `max_message_size`, `MAX_MESSAGE_SIZE`.
def maxMessage : Nat := 4096

-- The topic our frame carries, `TOPIC_ID_LEN`.
def topicLen : Nat := 32

-- Our frame limit, `FRAME_LIMIT`: gossip's size plus the topic.
def frameLimit : Nat := maxMessage + topicLen

-- What a broadcast adds to its content besides the content's length prefix,
-- `BROADCAST_OVERHEAD`: a tag, a 32-byte ID and the scope at its top round.
def overhead : Nat := 37

-- The bytes postcard's varint takes for `n`: seven bits per byte, up to the
-- ten bytes a 64-bit length can need.
def varintLen (n : Nat) : Nat :=
  -- Under 2^7: one byte;
  if n < 128 then 1
  -- under 2^14: two;
  else if n < 16384 then 2
  -- and so on, one more byte for every seven more bits.
  else if n < 2097152 then 3
  else if n < 268435456 then 4
  else if n < 34359738368 then 5
  else if n < 4398046511104 then 6
  else if n < 562949953421312 then 7
  else if n < 72057594037927936 then 8
  else if n < 9223372036854775808 then 9
  else 10

-- The size gossip checks a broadcast of `n` content bytes at.
def gossipSize (n : Nat) : Nat := overhead + varintLen n + n

-- Gossip sends a broadcast only within `max_message_size - 2`.
def gossipSends (n : Nat) : Prop := gossipSize n ≤ maxMessage - 2

-- The largest content gossip sends, `MAX_BROADCAST`.
def maxBroadcast : Nat := 4055

-- Promise: a length of 128 or more takes at least two varint bytes.
theorem varintLen_ge_two (n : Nat) (h : 128 ≤ n) : 2 ≤ varintLen n := by
  -- Every branch past the first gives two or more; the first contradicts `h`.
  unfold varintLen
  repeat' split
  all_goals omega

-- Promise: a broadcast gossip sends, at any round, is a frame within our
-- limit: the topic, the one-byte message tag, and a message no larger than
-- its size at the top round.
theorem broadcast_fits_frame (n message : Nat)
    -- Given gossip sends `n` bytes of content,
    (hsends : gossipSends n)
    -- and the message it sends is no larger than at the top round,
    (hround : message ≤ gossipSize n) :
    -- the frame fits our limit.
    topicLen + 1 + message ≤ frameLimit := by
  -- Spell out the sizes; what is left is arithmetic.
  unfold gossipSends at hsends
  simp only [frameLimit, topicLen, maxMessage] at *
  omega

-- Promise: any message gossip sends, a tag and body under its
-- `max_message_size`, is a frame within our limit once the topic is added.
theorem emitted_fits_frame (message : Nat)
    -- Given gossip keeps the message under its size,
    (h : message < maxMessage) :
    -- the frame with the topic fits our limit.
    topicLen + message ≤ frameLimit := by
  -- The limit is that size plus the topic.
  unfold frameLimit
  omega

-- Promise: gossip sends a broadcast of `MAX_BROADCAST` bytes.
theorem max_broadcast_fits : gossipSends maxBroadcast := by
  -- 37 + 2 + 4055 = 4094, which is 4096 - 2.
  unfold gossipSends
  decide

-- Promise: gossip drops every broadcast larger than `MAX_BROADCAST`, so a
-- publish over it is refused for good reason.
theorem over_max_broadcast_dropped (n : Nat)
    -- Given more content than `MAX_BROADCAST`,
    (h : maxBroadcast < n) :
    -- gossip does not send it.
    ¬ gossipSends n := by
  -- Such a length takes at least two varint bytes,
  have hlen := varintLen_ge_two n (by unfold maxBroadcast at h; omega)
  -- so its size is at least 37 + 2 + 4056, past 4094.
  unfold gossipSends gossipSize overhead maxMessage
  unfold maxBroadcast at h
  omega

end GossipFrame
