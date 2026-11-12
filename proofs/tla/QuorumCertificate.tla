---- MODULE QuorumCertificate ----
\* The story. Some resources need exactly one winner: one job claim, one
\* holder of a slot. Contenders each propose themselves for the slot. A fixed
\* set of validators votes, and a vote is a signed block. An honest validator
\* votes once per slot, for the first proposal it takes up. A cheating
\* validator may vote for every proposal.
\*
\* A certificate is Quorum votes for one proposal. A contender acts on the
\* slot (starts the job) only once it holds a certificate for itself; nobody
\* picks a winner, the certificate is the decision.
\*
\* What must never happen: two different proposals get certificates; two
\* contenders act on the slot. What must always happen: when only one
\* contender wants the slot, it ends up certified, even if a cheating
\* validator refuses to vote.
\*
\* Why it works: with N validators of which at most F cheat, any two sets of
\* Quorum votes share Quorum + Quorum - N validators. With Quorum = 2F + 1 and
\* N = 3F + 1 that is F + 1, so at least one honest validator is in both, and
\* it voted only once. Quorum must also be at most N - F, so the honest
\* validators alone can form one.
\*
\* A cheating validator's two votes for one slot are two signed blocks by one
\* key, neither building on the other: a fork, handled by ChainJail.tla. A
\* split vote (no proposal reaches Quorum) is left to the slow path, outside
\* this model.
\*
\* A restored validator is the trap. Its old vote still counts, it has
\* forgotten it, and its new key could vote again on the same slot, so one
\* machine would count twice. The rule: a re-admitted key abstains on every
\* slot that was already open when it was admitted. Abstaining spends fault
\* budget, so on those slots the restored validator counts towards F.
EXTENDS Naturals, FiniteSets

CONSTANTS
  Honest,          \* the honest validators
  Cheaters,        \* the cheating validators
  Contenders,      \* who may propose themselves for the slot
  Quorum,          \* votes needed for a certificate; GREEN 2F + 1
  VoteOnce,        \* GREEN TRUE; RED_Revote FALSE: an honest validator votes again
  ActOnCert,       \* GREEN TRUE; RED_Optimistic FALSE: a contender acts as soon as it proposes
  AbstainOnRestore, \* GREEN TRUE; RED_RestoreRevote FALSE: a restored validator votes again
  Restores         \* the scenario: whether an honest validator is ever restored from a backup

Validators == Honest \cup Cheaters   \* everyone who votes

ASSUME Honest \cap Cheaters = {}                     \* nobody is both
ASSUME Quorum \in Nat                                \* a count of votes
ASSUME VoteOnce \in BOOLEAN /\ ActOnCert \in BOOLEAN
ASSUME AbstainOnRestore \in BOOLEAN /\ Restores \in BOOLEAN

\* proposed: the contenders that have proposed themselves.
\* votes: every signed vote, as [voter, for].
\* acted: the contenders that acted on the slot.
\* restored: the honest validators restored from a backup taken before they
\*   voted. Each signs with a new, re-admitted key in the same seat, and has
\*   forgotten its vote, which still counts.
VARIABLES proposed, votes, acted, restored
vars == <<proposed, votes, acted, restored>>

TypeOK ==
  /\ proposed \subseteq Contenders                         \* only contenders propose
  /\ votes \subseteq [voter : Validators, for : Contenders]  \* well-formed votes
  /\ acted \subseteq Contenders                            \* only contenders act
  /\ restored \subseteq Honest                             \* only honest validators restore

\* The validators that voted for contender c.
VotersFor(c) == {x.voter : x \in {y \in votes : y.for = c}}

\* Whether contender c holds a certificate.
Certified(c) == Cardinality(VotersFor(c)) >= Quorum

\* Whether honest validator v remembers voting: a restore wipes the memory.
RemembersVote(v) == v \notin restored /\ \E x \in votes : x.voter = v

Init ==
  /\ proposed = {}                            \* nobody proposed yet
  /\ votes = {}                               \* no votes yet
  /\ acted = {}                               \* nobody acted
  /\ restored = {}                            \* nobody restored

\* Contender c proposes itself.
Propose(c) ==
  /\ c \notin proposed                        \* once
  /\ proposed' = proposed \cup {c}
  /\ UNCHANGED <<votes, acted, restored>>

\* Honest validator v votes for proposal c, once per slot.
Vote(v, c) ==
  /\ c \in proposed                           \* only for a real proposal
  /\ [voter |-> v, for |-> c] \notin votes    \* not this vote again
  /\ VoteOnce => ~RemembersVote(v)            \* RED_Revote drops this guard
  /\ AbstainOnRestore => v \notin restored     \* the slot was open at its re-admission
  /\ votes' = votes \cup {[voter |-> v, for |-> c]}
  /\ UNCHANGED <<proposed, acted, restored>>

\* A cheating validator votes for any proposal, as often as it likes.
CheatVote(v, c) ==
  /\ c \in proposed                           \* still only real proposals
  /\ [voter |-> v, for |-> c] \notin votes    \* a new vote
  /\ votes' = votes \cup {[voter |-> v, for |-> c]}
  /\ UNCHANGED <<proposed, acted, restored>>

\* Contender c acts on the slot.
Act(c) ==
  /\ c \in proposed                           \* it proposed
  /\ c \notin acted                           \* once
  /\ ActOnCert => Certified(c)                \* RED_Optimistic drops this guard
  /\ acted' = acted \cup {c}
  /\ UNCHANGED <<proposed, votes, restored>>

\* Honest validator v is restored from a backup taken before its vote: it
\* forgets the vote, which stays counted under its old key, and comes back
\* under a new key in the same seat.
Restore(v) ==
  /\ Restores                                 \* only in a restore scenario
  /\ v \notin restored                        \* once
  /\ \E x \in votes : x.voter = v             \* it had voted
  /\ restored' = restored \cup {v}
  /\ UNCHANGED <<proposed, votes, acted>>

Next ==
  \/ \E c \in Contenders : Propose(c)
  \/ \E v \in Honest, c \in Contenders : Vote(v, c)
  \/ \E v \in Cheaters, c \in Contenders : CheatVote(v, c)
  \/ \E c \in Contenders : Act(c)
  \/ \E v \in Honest : Restore(v)

\* Honest validators get round to voting, and a certified contender acts.
\* Contenders propose when they like; cheaters need never vote.
Fairness ==
  /\ \A v \in Honest : WF_vars(\E c \in Contenders : Vote(v, c))
  /\ \A c \in Contenders : WF_vars(Act(c))

Spec == Init /\ [][Next]_vars /\ Fairness

\* At most one proposal is ever certified.
INV_OneCertificate == \A c, d \in Contenders : Certified(c) /\ Certified(d) => c = d

\* At most one contender ever acts on the slot.
INV_OneActs == Cardinality(acted) <= 1

\* When exactly one contender has proposed, it ends up certified and acts.
PROP_UncontendedCommits ==
  \A c \in Contenders : (proposed = {c}) ~> (c \in acted \/ Cardinality(proposed) > 1)
====
