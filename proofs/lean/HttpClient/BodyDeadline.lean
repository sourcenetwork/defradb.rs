/-!
# HttpClient.BodyDeadline: a body stream past its time limit times out once, then ends

The story, in plain words. A response body can be read as a stream of pieces
(`Response::bytes_stream`). The request's time limit, when it has one, still
holds: each time the stream is asked for its next piece, it first looks at
the time limit; if it has passed, the stream answers with a timeout error and
lets go of the body, even when the body has a piece ready. Otherwise it asks
the body. After a timeout, every later ask answers that the stream has ended:
no piece of the body is read after its timeout, and the timeout is given once.

What must never happen:
* after a timeout, the stream gives anything but its end
  (`ends_after_timeout`), or gives a second timeout (`at_most_one_timeout`);
* an ask after the time limit passed gives a piece of the body, or waits
  (`deadline_wins_over_a_ready_piece`, `never_waits_past_the_deadline`);
* a stream with no time limit, or one whose limit has not passed, times out
  (`no_limit_no_timeout`, `no_timeout_before_the_deadline`).

The witnesses at the end show what the guards prevent: the code before the
first fix let go of the timer instead of the body, so the next ask read the
body on with no time limit at all; a stream that kept both would time out
again on every ask; and the code before the second fix looked at the time
limit only when the body had nothing ready, so a body that always had a piece
ready (a fast server and a slow reader) was read on past the limit.

Rust anchors (crates/net/src/http/response.rs): `BytesStream::poll_next`, and
`Response::bytes_stream`, which builds the stream with the body and a sleep
until the request's deadline. A frame that is not data (trailers) is skipped
and the body asked again, as the loop does. Hyper's body answers each
`poll_frame` with a data frame, another frame, an error, its end, or Pending.
-/

namespace HttpClient.BodyDeadline

-- One answer of hyper's body to `poll_frame`.
inductive Frame where
  -- A piece of the body;
  | data (piece : Nat)
  -- a frame that is not data, such as trailers;
  | trailers
  -- the body failed;
  | error
  -- the body's end;
  | finished
  -- nothing yet: Pending.
  | waiting
  deriving DecidableEq, Repr

-- What one `poll_next` answers.
inductive Out where
  -- `Ready(Some(Ok(piece)))`;
  | piece (p : Nat)
  -- `Ready(Some(Err(Error::Body)))`;
  | bodyError
  -- `Ready(Some(Err(Error::Timeout)))`;
  | timeout
  -- `Ready(None)`;
  | ended
  -- `Pending`.
  | pending
  deriving DecidableEq, Repr

-- The stream's state: whether it still holds the body (`body` is `Some`), and
-- whether it has a time limit (`sleep` is `Some`).
structure Stream where
  body : Bool
  sleep : Bool
  deriving DecidableEq, Repr

-- The loop inside one `poll_next`, once the time limit was looked at: it asks
-- the body, in turn, until it answers.
def loop (s : Stream) : List Frame → Stream × Out
  -- Not data: skip it and ask the body again;
  | .trailers :: rest => loop s rest
  -- a piece: give it;
  | .data p :: _ => (s, .piece p)
  -- an error: give it;
  | .error :: _ => (s, .bodyError)
  -- the end: give it;
  | .finished :: _ => (s, .ended)
  -- nothing yet: wait. (No more answers counts as nothing yet.)
  | .waiting :: _ | [] => (s, .pending)

-- `BytesStream::poll_next`, given what the body answers to each of the loop's
-- asks and whether the time limit has passed when the sleep is polled, which
-- is first: a stream that let go of its body has ended; one whose limit passed
-- times out, letting go of the body; otherwise the loop runs.
def poll (s : Stream) (frames : List Frame) (due : Bool) : Stream × Out :=
  if s.body then
    if s.sleep && due then ({ s with body := false }, .timeout) else loop s frames
  else (s, .ended)

-- A stream asked again and again: each ask with what the body answers in it
-- and whether the time limit has passed by then. It gives every answer.
def run (s : Stream) : List (List Frame × Bool) → List Out
  -- No ask: no answer.
  | [] => []
  -- One ask, then the rest from the stream it leaves.
  | (frames, due) :: rest => (poll s frames due).2 :: run (poll s frames due).1 rest

-- The promise, as a check over the answers: after the first timeout, every
-- answer is the end.
def endsAfterTimeout : List Out → Bool
  -- No answer: nothing to break;
  | [] => true
  -- the first timeout: every answer after it must be the end;
  | .timeout :: rest => rest.all (· == .ended)
  -- any other answer: look further.
  | _ :: rest => endsAfterTimeout rest

-- Promise: the loop leaves the stream as it was.
theorem loop_keeps_stream (s : Stream) : ∀ frames : List Frame, (loop s frames).1 = s := by
  intro frames
  induction frames with
  -- No answer: waiting, the stream unchanged.
  | nil => simp [loop]
  | cons f rest ih =>
    cases f with
    -- Skipped: the rest decides.
    | trailers => simpa [loop] using ih
    -- A piece, an error, the end, nothing yet: the stream unchanged.
    | data p => simp [loop]
    | error => simp [loop]
    | finished => simp [loop]
    | waiting => simp [loop]

-- Promise: the loop never gives a timeout; only the look at the limit does.
theorem loop_never_times_out (s : Stream) :
    ∀ frames : List Frame, (loop s frames).2 ≠ .timeout := by
  intro frames
  induction frames with
  -- No answer: waiting, which is Pending.
  | nil => simp [loop]
  | cons f rest ih =>
    cases f with
    -- Skipped: the rest decides.
    | trailers => simpa [loop] using ih
    -- A piece, an error, the end, nothing yet: not a timeout.
    | data p => simp [loop]
    | error => simp [loop]
    | finished => simp [loop]
    | waiting => simp [loop]

-- Promise: an ask gives a timeout only by letting go of the body.
theorem poll_timeout_drops (s : Stream) (frames : List Frame) (due : Bool)
    -- Given the ask answered a timeout,
    (h : (poll s frames due).2 = .timeout) :
    -- the stream it leaves holds no body.
    (poll s frames due).1.body = false := by
  -- Every case of the three flags: only the look at the limit times out.
  cases hb : s.body <;> cases hs : s.sleep <;> cases due <;>
    simp_all [poll, loop_never_times_out]

-- Promise: a stream that let go of its body answers its end, and stays so.
theorem dropped_stays_ended (s : Stream) (hs : s.body = false) :
    ∀ asks : List (List Frame × Bool), ∀ o ∈ run s asks, o = .ended := by
  intro asks
  induction asks generalizing s with
  -- No ask: no answer.
  | nil => intro o ho; simp [run] at ho
  | cons a rest ih =>
    intro o ho
    obtain ⟨frames, due⟩ := a
    -- This ask: the stream has no body, so it ends and stays the same.
    have hp : poll s frames due = (s, .ended) := by simp [poll, hs]
    simp only [run, hp, List.mem_cons] at ho
    rcases ho with h | h
    -- This ask's answer: the end.
    · exact h
    -- A later one: the stream is unchanged, so the same holds.
    · exact ih s hs o h

-- Promise: after the first timeout, every later answer is the stream's end.
theorem ends_after_timeout (s : Stream) :
    ∀ asks : List (List Frame × Bool), endsAfterTimeout (run s asks) = true := by
  intro asks
  induction asks generalizing s with
  -- No ask: nothing to break.
  | nil => rfl
  | cons a rest ih =>
    obtain ⟨frames, due⟩ := a
    simp only [run]
    -- Look at this ask's answer.
    by_cases ht : (poll s frames due).2 = .timeout
    -- A timeout: the body was let go of, so every later answer is the end.
    · rw [ht]
      have hb := poll_timeout_drops s frames due ht
      simp only [endsAfterTimeout, List.all_eq_true]
      intro o ho
      have := dropped_stays_ended _ hb rest o ho
      subst this; rfl
    -- Anything else: the promise is about the rest.
    · have hrest := ih (poll s frames due).1
      -- Whatever this answer is, not a timeout, the check moves on to the rest.
      generalize (poll s frames due).2 = o at ht ⊢
      cases o <;> simp_all [endsAfterTimeout]

-- Promise: a list whose answers after its first timeout are all ends holds at
-- most one timeout.
theorem one_timeout_of_ends : ∀ outs : List Out,
    endsAfterTimeout outs = true → outs.count .timeout ≤ 1
  -- No answer: no timeout.
  | [], _ => by simp
  -- The first timeout: none after it.
  | .timeout :: rest, h => by
    simp only [endsAfterTimeout, List.all_eq_true] at h
    -- Every later answer is the end, so none is a timeout.
    have : rest.count .timeout = 0 := by
      apply List.count_eq_zero.mpr
      intro hmem
      have := h _ hmem
      simp at this
    simp [List.count_cons, this]
  -- Any other answer: it is not a timeout, and the rest has at most one.
  | .piece p :: rest, h => by simp only [endsAfterTimeout] at h; simpa using one_timeout_of_ends rest h
  | .bodyError :: rest, h => by simp only [endsAfterTimeout] at h; simpa using one_timeout_of_ends rest h
  | .ended :: rest, h => by simp only [endsAfterTimeout] at h; simpa using one_timeout_of_ends rest h
  | .pending :: rest, h => by simp only [endsAfterTimeout] at h; simpa using one_timeout_of_ends rest h

-- Promise: a stream gives at most one timeout, whatever happens.
theorem at_most_one_timeout (s : Stream) (asks : List (List Frame × Bool)) :
    (run s asks).count .timeout ≤ 1 :=
  one_timeout_of_ends _ (ends_after_timeout s asks)

-- Promise: once the time limit has passed, an ask to a stream that still
-- holds its body times out, whatever the body has ready.
theorem deadline_wins_over_a_ready_piece (s : Stream) (frames : List Frame)
    -- Given the stream holds its body and has a time limit,
    (hb : s.body = true) (hs : s.sleep = true) :
    -- an ask after the limit passed answers the timeout.
    (poll s frames true).2 = .timeout := by
  -- The look at the limit comes first and finds it passed.
  simp [poll, hb, hs]

-- Promise: once the time limit has passed, an ask never waits: it answers (a
-- timeout, or the end).
theorem never_waits_past_the_deadline (s : Stream) (frames : List Frame)
    -- Given the stream has a time limit,
    (hs : s.sleep = true) :
    -- an ask after it passed does not answer Pending.
    (poll s frames true).2 ≠ .pending := by
  -- Holding the body, it times out; without it, it ended.
  cases hb : s.body <;> simp [poll, hb, hs]

-- Promise: an ask before the time limit passed never times out.
theorem no_timeout_before_the_deadline (s : Stream) (frames : List Frame) :
    (poll s frames false).2 ≠ .timeout := by
  -- Holding the body, the loop runs, and it never times out; without it, the end.
  cases hb : s.body <;> simp [poll, hb, loop_never_times_out]

-- Promise: an ask keeps the stream's time limit as it was.
theorem poll_keeps_sleep (s : Stream) (frames : List Frame) (due : Bool) :
    (poll s frames due).1.sleep = s.sleep := by
  -- A timeout changes only `body`; the loop changes nothing.
  cases hb : s.body <;> cases hs : s.sleep <;> cases due <;>
    simp [poll, hb, hs, loop_keeps_stream]

-- Promise: a stream with no time limit never times out.
theorem no_limit_no_timeout (s : Stream) (hs : s.sleep = false) :
    ∀ asks : List (List Frame × Bool), .timeout ∉ run s asks := by
  intro asks
  induction asks generalizing s with
  -- No ask: no answer.
  | nil => simp [run]
  | cons a rest ih =>
    obtain ⟨frames, due⟩ := a
    -- Not this answer, and not a later one.
    simp only [run, List.mem_cons, not_or]
    -- The stream still has no limit after this ask.
    have hkeep : (poll s frames due).1.sleep = false := by
      rw [poll_keeps_sleep]; exact hs
    refine ⟨?_, ih _ hkeep⟩
    -- And this ask cannot time out: with no limit, only the loop runs.
    intro h
    cases hb : s.body <;> simp [poll, hb, hs] at h
    · exact loop_never_times_out s frames h.symm

/-! ## What each guard prevents -/

-- A stream asked again and again through another `poll_next`, for the
-- witnesses below.
def runWith (step : Stream → List Frame → Bool → Stream × Out) (s : Stream) :
    List (List Frame × Bool) → List Out
  -- No ask: no answer.
  | [] => []
  -- One ask with `step`, then the rest.
  | (frames, due) :: rest => (step s frames due).2 :: runWith step (step s frames due).1 rest

-- RED (the code before the first fix): a timeout lets go of the timer, not
-- the body.
def pollOld (s : Stream) (frames : List Frame) (due : Bool) : Stream × Out :=
  if s.body then
    if s.sleep && due then ({ s with sleep := false }, .timeout) else loop s frames
  else (s, .ended)

-- Witness: the old stream times out, then reads a piece of the body past the
-- limit on the next ask.
theorem old_reads_after_timeout :
    runWith pollOld ⟨true, true⟩ [([.waiting], true), ([.data 5], true)] = [.timeout, .piece 5] ∧
      endsAfterTimeout [.timeout, .piece 5] = false := by
  -- Run the two asks: a timeout, then a piece.
  decide

-- RED: a timeout that lets go of nothing.
def pollKeep (s : Stream) (frames : List Frame) (due : Bool) : Stream × Out :=
  if s.body then
    if s.sleep && due then (s, .timeout) else loop s frames
  else (s, .ended)

-- Witness: a stream that keeps both times out on every ask: two timeouts.
theorem keep_times_out_twice :
    runWith pollKeep ⟨true, true⟩ [([.waiting], true), ([.waiting], true)] =
      [.timeout, .timeout] := by
  -- Run two asks past the limit: two timeouts.
  decide

-- RED (the code before the second fix): the limit is looked at only when the
-- body has nothing ready.
def pollLate (s : Stream) (frames : List Frame) (due : Bool) : Stream × Out :=
  if s.body then
    match loop s frames with
    -- Nothing ready: now the limit is looked at.
    | (_, .pending) => if s.sleep && due then ({ s with body := false }, .timeout) else (s, .pending)
    -- Anything else is given, the limit unseen.
    | answer => answer
  else (s, .ended)

-- Witness: past the limit, a body that always has a piece ready is read to
-- its end, so `ClientBuilder::timeout`'s "from connecting to the end of the
-- response body" would not hold.
theorem late_reads_past_the_deadline :
    runWith pollLate ⟨true, true⟩ [([.data 1], true), ([.data 2], true), ([.finished], true)] =
      [.piece 1, .piece 2, .ended] := by
  -- Run the three asks, all past the limit: both pieces, then the end.
  decide

-- And the shipped stream, on the same asks, times out once and then ends.
theorem shipped_times_out_once :
    run ⟨true, true⟩ [([.data 1], true), ([.data 2], true), ([.finished], true)] =
      [.timeout, .ended, .ended] := by
  -- Run the three asks: one timeout, then the end twice.
  decide

end HttpClient.BodyDeadline
