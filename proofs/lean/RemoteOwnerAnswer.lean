/-! Which owner a transaction is told on a remote document ACP provider, and
when the transaction may commit on it.

An owed registration (this transaction's, or an earlier committed one's)
names its creator: the Node's own claim, the rule reads and verified writes
already apply. Otherwise the provider's owner, read at the edge and recorded
in the transaction. The commit runs only when every recorded provider answer
equals the provider's answer to the same request read again at commit; any
other outcome is the "owner changed" conflict. This models
`query::txn::ObservedOwners` (`record`, `confirm`) and
`QueryRunner::remote_owner`; `RemoteOwnerObservation.tla` models the order of
reads, settles and commits around it.
-/
namespace RemoteOwnerAnswer

-- A prepared request, by its exact bytes, and the provider's exact answer.
abbrev Request := String
abbrev Owner := String

-- Where the answer came from.
inductive Source where
  | owed
  | provider
  deriving DecidableEq, Repr

structure Answer where
  owner : Option Owner
  source : Source
  deriving DecidableEq, Repr

-- The owed creator wins; the provider answers only when nothing is owed.
def answer (owedCreator : Option Owner) (provider : Option Owner) : Answer :=
  match owedCreator with
  | some creator => ⟨some creator, .owed⟩
  | none => ⟨provider, .provider⟩

theorem owed_names_creator (creator : Owner) (provider : Option Owner) :
    answer (some creator) provider = ⟨some creator, .owed⟩ := rfl

theorem provider_only_when_not_owed (provider : Option Owner) :
    answer none provider = ⟨provider, .provider⟩ := rfl

-- An owed registration is never reported as unowned, whatever the provider says.
theorem owed_never_unowned (creator : Owner) (provider : Option Owner) :
    (answer (some creator) provider).owner ≠ none := by simp [answer]

-- What a transaction recorded: every provider answer it used, in order.
abbrev Recorded := List (Request × Option Owner)

-- What the confirmation round read at commit, by request.
abbrev Confirmations := List (Request × Option Owner)

def confirmedAs (confirmations : Confirmations) (request : Request) : Option (Option Owner) :=
  (confirmations.find? (fun entry => entry.1 = request)).map Prod.snd

inductive Verdict where
  | commit
  | ownerChanged
  deriving DecidableEq, Repr

-- Every recorded answer must be confirmed as exactly itself.
def holds (confirmations : Confirmations) (entry : Request × Option Owner) : Bool :=
  confirmedAs confirmations entry.1 == some entry.2

def verdict (recorded : Recorded) (confirmations : Confirmations) : Verdict :=
  if recorded.all (holds confirmations) then .commit else .ownerChanged

-- A transaction that used no provider answer commits without a round.
theorem nothing_recorded_commits (confirmations : Confirmations) :
    verdict [] confirmations = .commit := rfl

-- A commit means every recorded answer was confirmed as itself.
theorem commit_only_if_confirmed (recorded : Recorded) (confirmations : Confirmations)
    (h : verdict recorded confirmations = .commit) :
    ∀ entry ∈ recorded, confirmedAs confirmations entry.1 = some entry.2 := by
  intro entry hmem
  unfold verdict at h
  split at h
  · rename_i hall
    have := List.all_eq_true.mp hall entry hmem
    simpa [holds] using this
  · cases h

-- One answer the provider no longer gives is enough to refuse the commit.
theorem changed_answer_conflicts (recorded : Recorded) (confirmations : Confirmations)
    (entry : Request × Option Owner) (hmem : entry ∈ recorded)
    (hchanged : confirmedAs confirmations entry.1 ≠ some entry.2) :
    verdict recorded confirmations = .ownerChanged := by
  unfold verdict
  split
  · rename_i hall
    have := List.all_eq_true.mp hall entry hmem
    exact absurd (by simpa [holds] using this) hchanged
  · rfl

-- An unconfirmed answer (the round did not read it) never commits.
theorem unconfirmed_conflicts (recorded : Recorded) (confirmations : Confirmations)
    (entry : Request × Option Owner) (hmem : entry ∈ recorded)
    (hmissing : confirmedAs confirmations entry.1 = none) :
    verdict recorded confirmations = .ownerChanged :=
  changed_answer_conflicts recorded confirmations entry hmem (by simp [hmissing])

-- Two different answers to one request (the provider changed between two
-- observations in the transaction) can never both be confirmed.
theorem split_answers_conflict (recorded : Recorded) (confirmations : Confirmations)
    (request : Request) (first second : Option Owner) (hdiff : first ≠ second)
    (h1 : (request, first) ∈ recorded) (h2 : (request, second) ∈ recorded) :
    verdict recorded confirmations = .ownerChanged := by
  by_cases hfirst : confirmedAs confirmations request = some first
  · exact changed_answer_conflicts recorded confirmations (request, second) h2
      (by simp only; rw [hfirst]; intro heq; exact hdiff (Option.some.inj heq))
  · exact changed_answer_conflicts recorded confirmations (request, first) h1 hfirst

end RemoteOwnerAnswer
