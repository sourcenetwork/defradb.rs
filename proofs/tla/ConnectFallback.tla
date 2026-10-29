---- MODULE ConnectFallback ----
\* The story, in plain words.
\*
\* The HTTP client opens a TCP connection to a host whose name resolved to
\* addresses of two families, IPv6 and IPv4. It splits them by family: the
\* family of the first address is the preferred one, the other is the
\* fallback. Each family is tried as one attempt: its addresses one after
\* another, the first that takes the connection wins, or the last refusal is
\* its error. Then (first_connected):
\*   * the preferred attempt starts at once, alone;
\*   * if it connects within 300 ms (the fallback delay), its socket is the
\*     answer, and the fallback never starts;
\*   * if it fails within the delay, the fallback starts at once, and its
\*     answer is the answer;
\*   * once the delay passes, the fallback starts beside it, and the two
\*     race: the first to connect wins and the other is dropped (its socket,
\*     if it had one, is closed); the first to fail leaves the other to
\*     finish, and its answer is the answer;
\*   * with one family only, its attempt is the answer.
\* The client's connect timeout, when one is set, covers the whole of it:
\* when it passes, every attempt is dropped and the connect fails.
\*
\* A tiny example. IPv6 first: its route drops packets, so its attempt hangs.
\* At 300 ms the IPv4 attempt starts and connects; the IPv6 attempt is
\* dropped, and the IPv4 socket goes to the pool.
\*
\* What must never happen:
\*   1. the answer is not one socket: a socket other than the winner's is
\*      left open, an attempt still runs after the answer, or the winner is
\*      not the attempt that connected - INV_OneSocket;
\*   2. the fallback starts before the delay while the preferred family has
\*      not failed - INV_FallbackWaits;
\*   3. the connect waits with no attempt under way - INV_NeverIdle;
\*   4. an error is the answer while some family has not failed
\*      - INV_ErrorOnlyWhenAllFail;
\*   5. the error is not that of the last attempt the connect saw fail
\*      - INV_LastError;
\*   6. with a connect timeout set, the connect runs past it
\*      - INV_WithinLimit;
\*   7. the connect never ends - LIVE_Ends.
\*
\* Correspondence with the Rust code (crates/net/src/connect.rs):
\*
\*   | Model            | Rust                                                       |
\*   |------------------|------------------------------------------------------------|
\*   | Fams, "P", "F"   | first_connected's partition by family (connect.rs:155-158):|
\*   |                  | `preferred` and `fallback`                                 |
\*   | att[f] = "run"   | the attempt's TcpStream::connect future runs: nuclei tries |
\*   |                  | the family's addresses in order (nuclei net/tcp.rs connect)|
\*   | att[f] = "idle"  | the fallback future pinned but never polled (connect.rs:   |
\*   |                  | 163): an async connect does nothing until polled           |
\*   | "ok", "bad"      | the attempt connected, or failed, and the task has not     |
\*   |                  | polled it since                                            |
\*   | "won", "err"     | the task took that answer                                  |
\*   | "dropped"        | the future dropped when first_connected returns (or when   |
\*   |                  | the connect timeout drops `connecting`): its socket closes |
\*   | pc = "single"    | `if fallback.is_empty() { return preferred.await }`        |
\*   |                  | (connect.rs:160-162)                                       |
\*   | pc = "first"     | timeout(FALLBACK_DELAY, preferred) (connect.rs:164-168)    |
\*   | pc = "race"      | nuclei::select! { biased; ... } (connect.rs:169-179)       |
\*   | pc = "onlyF"     | `Ok(Err(_)) => return fallback.await` (connect.rs:166) and |
\*   |                  | `Err(_) => fallback.await` (connect.rs:173)                |
\*   | pc = "onlyP"     | `Err(_) => preferred.await` (connect.rs:177)               |
\*   | pc = "wait"      | RED only: the delay waited out after a failure (no Rust)   |
\*   | D                | FALLBACK_DELAY (connect.rs:28), 300 ms                     |
\*   | CT, Limited      | Connector::connect_timeout and the nuclei::time::timeout   |
\*   |                  | around `connecting` (connect.rs:129-134)                   |
\*   | res              | first_connected's (or Connector::socket's) answer          |
\*   | fStart, lastSeen | history only, to state INV_FallbackWaits and INV_LastError |
\*   | DelayFallback    | the fallback polled only after the delay or a failure      |
\*   |                  | (RED: both started at once)                                |
\*   | StartOnFailure   | connect.rs:166 (RED: a failure within the delay waits the  |
\*   |                  | delay out)                                                 |
\*   | AwaitOther       | connect.rs:173 and :177 (RED: the first failure is the     |
\*   |                  | answer)                                                    |
\*   | KeepLastError    | the other's answer is returned whole (RED: the earlier     |
\*   |                  | error kept)                                                |
\*   | DropLosers       | the pinned futures drop with first_connected's frame (RED: |
\*   |                  | a loser left running, as a spawned task would be)          |
\*   | TimeoutAll       | the timeout wraps `connecting`, the whole race (RED: it    |
\*   |                  | covers only the name lookup)                               |
\*   | MayHang          | an attempt may never end by itself (a route that drops     |
\*   |                  | packets, before the OS gives up)                           |
\*
\* Gaps, explained:
\*   * The name lookup (Connector::socket, connect.rs:113-118) is left out:
\*     it runs before first_connected, under the same timeout.
\*   * A family's addresses are one attempt: nuclei's TcpStream::connect tries
\*     them in order and answers the first connection or the last error.
\*   * One step of the task is one poll: it sees every attempt that ended
\*     since the last one. Two attempts that ended before one poll are seen
\*     in the select's biased order, the preferred family first, so "the last
\*     to fail" is the last the task saw fail; two failures one poll sees
\*     count as at once.
\*   * A timer fires on time: the clock does not pass a due timer while the
\*     task waits on it (the delay, the connect timeout).

EXTENDS Naturals
\* Naturals gives numbers for the clock.

CONSTANTS D, CT, MaxTime, HasFallback, MayHang, Limited,
          DelayFallback, StartOnFailure, AwaitOther, KeepLastError,
          DropLosers, TimeoutAll
\* D is the fallback delay and CT the connect timeout, in clock ticks;
\* MaxTime is how far the clock may run. HasFallback says whether the name
\* resolved to both families, MayHang whether an attempt may never end by
\* itself, Limited whether a connect timeout is set. The six other switches
\* are guards in the code: TRUE as written, and each RED configuration
\* turns one off.

ASSUME /\ D \in 1..3
       \* the delay is a few ticks;
       /\ CT \in 1..4
       \* so is the connect timeout;
       /\ MaxTime \in 1..5
       \* the clock runs a few ticks at most;
       /\ \A s \in {HasFallback, MayHang, Limited, DelayFallback, StartOnFailure,
                    AwaitOther, KeepLastError, DropLosers, TimeoutAll} : s \in BOOLEAN
       \* and every switch is on or off.

Fams == IF HasFallback THEN {"P", "F"} ELSE {"P"}
\* The attempts: the preferred family, and the fallback when there is one.

Other(f) == IF f = "P" THEN "F" ELSE "P"
\* The attempt that is not f.

VARIABLES now, pc, att, res, fStart, lastSeen
\* now: the clock. pc: where the connect is. att: each attempt's state.
\* res: the answer, once there is one. fStart: why the fallback started
\* ("none" yet, after the "delay", after a "failure", or "early").
\* lastSeen: the last attempt whose failure the connect saw.

vars == <<now, pc, att, res, fStart, lastSeen>>
\* Every variable, for the stuttering steps.

Live == {"run", "ok", "bad"}
\* The states of an attempt under way: running, or ended and not yet seen.

TypeOK ==
  \* Every variable holds a value of its kind.
  /\ now \in 0..MaxTime
  \* the clock is within its bound;
  /\ pc \in {"single", "first", "wait", "race", "onlyF", "onlyP", "done"}
  \* the connect is at one of its waits ("wait" only in a RED), or done;
  /\ att \in [Fams -> {"idle", "run", "ok", "bad", "won", "err", "dropped"}]
  \* each attempt is in one of its states;
  /\ res \in {"none", "P", "F", "errP", "errF", "timeout"}
  \* the answer is a family's socket or error, a timeout, or none yet;
  /\ fStart \in {"none", "delay", "failure", "early"}
  \* the fallback started for one of these reasons, or not yet;
  /\ lastSeen \in {"none", "P", "F"}
  \* and the last failure seen is a family's, or none.

Init ==
  \* The connect begins at time zero:
  /\ now = 0
  /\ pc = CASE ~HasFallback -> "single"
  \* one family: its attempt alone;
              [] DelayFallback -> "first"
  \* two: the preferred alone, for the delay;
              [] OTHER -> "race"
  \* (RED: both at once);
  /\ att = [f \in Fams |-> IF f = "P" \/ ~DelayFallback THEN "run" ELSE "idle"]
  \* the preferred attempt runs, the fallback waits (RED: runs too);
  /\ res = "none"
  /\ fStart = IF HasFallback /\ ~DelayFallback THEN "early" ELSE "none"
  \* (RED: the fallback started early);
  /\ lastSeen = "none"
  \* and no failure seen.

TimeoutArmed == Limited /\ TimeoutAll /\ pc # "done"
\* The connect timeout is set, covers the race, and the connect is not over.

DelayDue == pc \in {"first", "wait"} /\ now >= D
\* The fallback delay has passed while the connect waits on it.

Tick ==
  \* Time passes, unless a timer the connect waits on is due: then it fires
  \* first.
  /\ pc # "done"
  /\ now < MaxTime
  /\ ~DelayDue
  /\ ~(TimeoutArmed /\ now >= CT)
  /\ now' = now + 1
  /\ UNCHANGED <<pc, att, res, fStart, lastSeen>>

Complete(f, connected) ==
  \* Attempt f ends by itself: it connected, or every address refused.
  /\ att[f] = "run"
  /\ att' = [att EXCEPT ![f] = IF connected THEN "ok" ELSE "bad"]
  /\ UNCHANGED <<now, pc, res, fStart, lastSeen>>

Finish(r, kept) ==
  \* The connect answers r. The attempt `kept` (if any) is the winner:
  /\ pc' = "done"
  /\ res' = r
  /\ att' = [f \in Fams |->
               CASE f = kept -> "won"
  \* the winner's socket is the answer;
                 [] att[f] \in {"bad", "err"} -> "err"
  \* an attempt that failed holds nothing;
                 [] att[f] \in {"run", "ok"} /\ DropLosers -> "dropped"
  \* any other attempt under way is dropped, and its socket closed (RED: left
  \* running);
                 [] OTHER -> att[f]]
  \* and one never started stays so.

StartFallback(why) ==
  \* The fallback attempt is polled for the first time, and starts.
  /\ att' = [att EXCEPT !["F"] = "run", !["P"] = IF att["P"] = "bad" THEN "err" ELSE att["P"]]
  /\ fStart' = why

Single ==
  \* One family: its attempt's answer is the answer.
  /\ pc = "single"
  /\ att["P"] \in {"ok", "bad"}
  /\ IF att["P"] = "ok" THEN Finish("P", "P") ELSE Finish("errP", "none")
  /\ lastSeen' = IF att["P"] = "bad" THEN "P" ELSE lastSeen
  /\ UNCHANGED <<now, fStart>>

First ==
  \* Within the delay, the preferred attempt ends, and the timeout's poll of
  \* it (the future first) sees it.
  /\ pc = "first"
  /\ att["P"] \in {"ok", "bad"}
  /\ IF att["P"] = "ok"
     \* It connected: its socket is the answer; the fallback never started.
     THEN /\ Finish("P", "P")
          /\ UNCHANGED <<fStart, lastSeen>>
     \* It failed: the fallback starts at once (RED: after the delay).
     ELSE /\ lastSeen' = "P"
          /\ res' = res
          /\ IF StartOnFailure
             THEN /\ StartFallback("failure")
                  /\ pc' = "onlyF"
             ELSE /\ att' = [att EXCEPT !["P"] = "err"]
                  /\ pc' = "wait"
                  /\ UNCHANGED fStart
  /\ UNCHANGED now

DelayFires ==
  \* The fallback delay passes with the preferred attempt still running (or,
  \* in the RED, after it failed): the fallback starts.
  /\ DelayDue
  /\ pc = "first" => att["P"] = "run"
  /\ StartFallback("delay")
  /\ pc' = IF pc = "first" THEN "race" ELSE "onlyF"
  /\ UNCHANGED <<now, res, lastSeen>>

Race ==
  \* The biased select is polled with at least one attempt ended: the
  \* preferred one is looked at first.
  /\ pc = "race"
  /\ \E f \in Fams : att[f] \in {"ok", "bad"}
  /\ LET f == IF att["P"] \in {"ok", "bad"} THEN "P" ELSE "F"
         \* the attempt the select takes;
     IN IF att[f] = "ok"
        \* It connected: the answer, and the other is dropped.
        THEN /\ Finish(f, f)
             /\ UNCHANGED lastSeen
        \* It failed: wait for the other (RED: answer this error).
        ELSE /\ lastSeen' = f
             /\ IF AwaitOther
                THEN /\ att' = [att EXCEPT ![f] = "err"]
                     /\ pc' = IF f = "P" THEN "onlyF" ELSE "onlyP"
                     /\ res' = res
                ELSE /\ Finish(IF f = "P" THEN "errP" ELSE "errF", "none")
  /\ UNCHANGED <<now, fStart>>

Only(f, nm) ==
  \* The connect waits on attempt f alone, the other having failed; f ends.
  /\ pc = nm
  /\ att[f] \in {"ok", "bad"}
  /\ IF att[f] = "ok"
     \* It connected: the answer.
     THEN /\ Finish(f, f)
          /\ UNCHANGED lastSeen
     \* It failed too: its error is the answer (RED: the earlier one).
     ELSE /\ lastSeen' = f
          /\ Finish(IF KeepLastError = (f = "P") THEN "errP" ELSE "errF", "none")
  /\ UNCHANGED <<now, fStart>>

TimeOut ==
  \* The connect timeout passes: every attempt is dropped and the connect
  \* fails.
  /\ TimeoutArmed
  /\ now >= CT
  /\ pc' = "done"
  /\ res' = "timeout"
  /\ att' = [f \in Fams |-> CASE att[f] \in {"run", "ok"} -> "dropped"
                             [] att[f] = "bad" -> "err"
                             [] OTHER -> att[f]]
  /\ UNCHANGED <<now, fStart, lastSeen>>

Task == Single \/ First \/ DelayFires \/ Race \/ Only("F", "onlyF") \/ Only("P", "onlyP") \/ TimeOut
\* The connect's own steps: one poll, or a timer.

Ends(f) == \E c \in BOOLEAN : Complete(f, c)
\* Attempt f ends, one way or the other.

Next == Task \/ Tick \/ (\E f \in Fams : Ends(f))
\* Each step is the connect's, the clock's, or an attempt's.

OsEndsAttempts == IF MayHang THEN TRUE ELSE \A f \in Fams : WF_vars(Ends(f))
\* Unless an attempt may hang, the OS ends every attempt it runs (its own
\* connect timeout, when nothing answers).

Spec == /\ Init /\ [][Next]_vars
        /\ WF_vars(Task) /\ WF_vars(Tick) /\ OsEndsAttempts
\* The connect is polled whenever it can go on, time passes, and the OS
\* ends the attempts (unless they may hang).

INV_OneSocket ==
  pc = "done" => \A f \in Fams : /\ att[f] \notin {"run", "ok"}
                                 \* nothing runs on or holds a socket,
                                 /\ (att[f] = "won") <=> (res = f)
                                 \* except the attempt answered, which connected.
\* Promise: once the connect answered, nothing it started still runs or
\* holds a socket, except the one socket it answered with, which is the
\* attempt that connected.

INV_FallbackWaits == fStart # "early"
\* Promise: the fallback never starts before the delay, unless the preferred
\* family failed first.

INV_NeverIdle == pc # "done" => \E f \in Fams : att[f] \in Live
\* Promise: while the connect waits, some attempt is under way.

INV_ErrorOnlyWhenAllFail == res \in {"errP", "errF"} => \A f \in Fams : att[f] = "err"
\* Promise: an error is the answer only once every family failed.

INV_LastError == /\ res = "errP" => lastSeen = "P"
                 /\ res = "errF" => lastSeen = "F"
\* Promise: the error answered is that of the last attempt the connect saw
\* fail.

INV_WithinLimit == (Limited /\ pc # "done") => now <= CT
\* Promise: with a connect timeout set, the connect never runs past it.

LIVE_Ends == <>(pc = "done")
\* Promise: every connect ends, with a socket or an error.

====
