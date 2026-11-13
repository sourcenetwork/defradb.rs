---- MODULE PeerKeyLoad ----
\* The story, in plain words.
\*
\* A node needs one peer key: the identity other peers know it by. Its store
\* is started again and again, and each start finds the key one of three
\* ways:
\*   * plain: no key option. The node uses the key in its store, making one
\*     there on the first start;
\*   * keyring: an application keyring holds the key. The store records the
\*     key's identity, so the node always comes back with that keyring;
\*   * start key: the caller hands in a key for this start only, as Go's
\*     P2PPrivateKey does. The node uses it as given.
\*
\* A start key must behave like an option of that one start. It is never
\* refused because of what earlier starts left in the store, and it leaves
\* the store exactly as it found it, so a later start without it carries on
\* with the store's own key as if the start key had never been used.
\*
\* What must never happen:
\*   1. a start with a start key is refused - INV_StartKeyStarts;
\*   2. a plain start is refused because an earlier start key left its mark
\*      in the store - INV_StartKeyLeavesNoMark;
\*   3. the key a plain start uses changes because a start key was used in
\*      between - INV_PlainKeyStable.
\*
\* Each guard has a RED configuration that removes it:
\*   * StartKeyUnchecked: a start key skips the checks the keyring path makes
\*     against the store (Red_StartKeyChecked);
\*   * StartKeyNotRecorded: a start key records nothing in the store
\*     (Red_StartKeyRecorded).
\*
\* Rust: crates/defra-node/src/peer_key.rs load_with_keyring, load_or_create,
\* StartPeerKey; crates/defra-node/src/libp2p_runtime.rs assemble_libp2p and
\* crates/defra-node/src/p2p_runtime.rs setup_iroh, which pass the start key
\* in; crates/ffi/src/node/mod.rs, which maps p2p_private_key onto it.
EXTENDS Naturals

CONSTANTS
  Keys,                \* the keys a keyring or a caller may bring
  None,                \* stands for "no key here"
  MaxStarts,           \* how many starts a run makes (bounds TLC)
  StartKeyUnchecked,   \* TRUE: a start key is not checked against the store
  StartKeyNotRecorded  \* TRUE: a start key writes nothing to the store

\* A key, or none.
KeyOrNone == Keys \cup {None}

VARIABLES
  plain,      \* the plaintext key the store holds, or None
  pinned,     \* the keyring identity the store records, or None
  starts,     \* how many starts have happened
  marked,     \* TRUE once a start key wrote to the store
  refusedSK,  \* TRUE once a start with a start key was refused
  refusedMark,\* TRUE once a plain start was refused over a start key's mark
  plainKey,   \* the key the last successful plain start used, or None
  plainMoved  \* TRUE once a plain start used another key than the one before

vars == <<plain, pinned, starts, marked, refusedSK, refusedMark, plainKey, plainMoved>>

\* Every variable has the kind of value it should.
TypeOK ==
  /\ plain \in KeyOrNone
  /\ pinned \in KeyOrNone
  /\ starts \in 0..MaxStarts
  /\ marked \in BOOLEAN
  /\ refusedSK \in BOOLEAN
  /\ refusedMark \in BOOLEAN
  /\ plainKey \in KeyOrNone
  /\ plainMoved \in BOOLEAN

\* A new store holds nothing, and nothing has happened yet.
Init ==
  /\ plain = None
  /\ pinned = None
  /\ starts = 0
  /\ marked = FALSE
  /\ refusedSK = FALSE
  /\ refusedMark = FALSE
  /\ plainKey = None
  /\ plainMoved = FALSE

\* Another start may happen while the run has starts left.
CanStart == starts < MaxStarts

\* A plain start (load_or_create). A store that records a keyring identity
\* refuses it (PeerKeyringRequired); otherwise it uses the store's key,
\* making one with key k on the first start.
PlainStart(k) ==
  /\ CanStart
  /\ starts' = starts + 1
  /\ IF pinned # None
     THEN \* Refused; remember when it was a start key's mark that refused it.
          /\ refusedMark' = (refusedMark \/ marked)
          /\ UNCHANGED <<plain, pinned, marked, refusedSK, plainKey, plainMoved>>
     ELSE LET used == IF plain = None THEN k ELSE plain
          IN /\ plain' = used
             \* The key moved when an earlier plain start used another one.
             /\ plainMoved' = (plainMoved \/ (plainKey # None /\ plainKey # used))
             /\ plainKey' = used
             /\ UNCHANGED <<pinned, marked, refusedSK, refusedMark>>

\* A keyring start with key k (load_with_keyring with a keyring). A store
\* with a plaintext key refuses it (PlaintextPeerKey), as does one that
\* records another identity (KeyringIdentityMismatch); otherwise the store
\* records k's identity.
KeyringStart(k) ==
  /\ CanStart
  /\ starts' = starts + 1
  /\ IF plain # None \/ (pinned # None /\ pinned # k)
     THEN UNCHANGED <<plain, pinned, marked, refusedSK, refusedMark, plainKey, plainMoved>>
     ELSE /\ pinned' = k
          /\ UNCHANGED <<plain, marked, refusedSK, refusedMark, plainKey, plainMoved>>

\* A start with the start key k (StartPeerKey). With StartKeyUnchecked off it
\* is checked as a keyring start is, and may be refused; with
\* StartKeyNotRecorded off it records its identity as a keyring start does.
StartKeyStart(k) ==
  /\ CanStart
  /\ starts' = starts + 1
  /\ IF ~StartKeyUnchecked /\ (plain # None \/ (pinned # None /\ pinned # k))
     THEN /\ refusedSK' = TRUE
          /\ UNCHANGED <<plain, pinned, marked, refusedMark, plainKey, plainMoved>>
     ELSE /\ pinned' = IF StartKeyNotRecorded THEN pinned ELSE k
          /\ marked' = (marked \/ ~StartKeyNotRecorded)
          /\ UNCHANGED <<plain, refusedSK, refusedMark, plainKey, plainMoved>>

Next ==
  \E k \in Keys :
    \/ PlainStart(k)
    \/ KeyringStart(k)
    \/ StartKeyStart(k)

Spec == Init /\ [][Next]_vars

\* ---- what must hold in every state ----

\* A start with a start key always starts.
INV_StartKeyStarts == ~refusedSK

\* No plain start is refused over a mark a start key left in the store.
INV_StartKeyLeavesNoMark == ~refusedMark

\* Every successful plain start uses the same key as the one before it.
INV_PlainKeyStable == ~plainMoved
====
