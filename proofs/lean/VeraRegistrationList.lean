/-!
A list of registrations, as one vera.rs request.

The registrations one creator owes go to vera.rs as one `bearerPolicyCmds`
request: an ordered list of commands under one token, which vera.rs applies
in order, all or none. The node builds that request from the objects to
register: each object's policy identifier is read, each object becomes one
registration command, and the request is refused before anything is sent
when it holds no object, more than vera.rs takes in one list (64), or more
bytes than a request may take.

What must never happen: a request is built with no command or more than 64,
a request is built over the byte bound, the commands are in another order
than the objects or name other objects, or a policy identifier that does not
read is sent at all. And a request within every bound is not refused.

Rust owner: `calldata` in
`crates/engine/vera/src/vera_rs/provider/registration_list.rs`, which reads
each policy with `vera_client::parse_policy_id` and checks the encoded size
against `MAX_OWED_REQUEST_BYTES`.

The model leaves the policy reader and the encoding abstract: `parse` stands
for `parse_policy_id` and `size` for the encoded request's length. The
theorems hold whatever they are.
-/
namespace VeraRegistrationList

/-- The most commands vera.rs applies in one list (`MAX_POLICY_COMMAND_LIST`). -/
def maxList : Nat := 64

/-- An object to register: its policy identifier as given, and the object. -/
abbrev Obj := String × String

/-- One registration command: the policy as read, and the object. -/
abbrev Cmd := String × String

/-- Every object's command, in order; `none` when a policy does not read. -/
def commands (parse : String → Option String) : List Obj → Option (List Cmd)
  -- No objects are no commands.
  | [] => some []
  -- The first object's policy is read, then the rest follow.
  | (policy, object) :: rest =>
      match parse policy, commands parse rest with
      -- Both read: this command, then the others.
      | some read, some others => some ((read, object) :: others)
      -- Either did not: nothing is built.
      | _, _ => none

/-- The request for `objects`, or `none` when it is refused before sending. -/
def request (parse : String → Option String) (size : List Cmd → Nat)
    (maxBytes : Nat) (objects : List Obj) : Option (List Cmd) :=
  -- An empty list or one longer than vera.rs takes is refused first.
  if objects.length = 0 ∨ maxList < objects.length then none
  else
    match commands parse objects with
    -- A policy that does not read refuses the whole request.
    | none => none
    -- Built, it is kept only within the byte bound.
    | some cmds => if size cmds ≤ maxBytes then some cmds else none

/-- Built commands name the objects in their order. -/
theorem commands_objects (parse : String → Option String) :
    ∀ (objects : List Obj) (cmds : List Cmd),
      commands parse objects = some cmds →
        cmds.map Prod.snd = objects.map Prod.snd := by
  -- By the objects, one at a time.
  intro objects
  induction objects with
  | nil =>
      -- No objects build no commands.
      intro cmds h
      simp [commands] at h
      subst h
      rfl
  | cons head rest ih =>
      -- The first command names the first object, the rest by induction.
      intro cmds h
      obtain ⟨policy, object⟩ := head
      simp only [commands] at h
      split at h
      · rename_i read others hp hr
        simp at h
        subst h
        simp [ih others hr]
      · simp at h

/-- A built request holds at least one command and at most `maxList`. -/
theorem request_count (parse : String → Option String) (size : List Cmd → Nat)
    (maxBytes : Nat) (objects : List Obj) (cmds : List Cmd)
    (h : request parse size maxBytes objects = some cmds) :
    1 ≤ cmds.length ∧ cmds.length ≤ maxList := by
  -- The command list is as long as the object list, which passed the bounds.
  unfold request at h
  split at h
  · simp at h
  · rename_i bounds
    split at h
    · simp at h
    · rename_i built hb
      split at h
      · simp at h
        subst h
        have hl := congrArg List.length (commands_objects parse objects built hb)
        simp at hl
        omega
      · simp at h

/-- A built request names the objects, in their order. -/
theorem request_order (parse : String → Option String) (size : List Cmd → Nat)
    (maxBytes : Nat) (objects : List Obj) (cmds : List Cmd)
    (h : request parse size maxBytes objects = some cmds) :
    cmds.map Prod.snd = objects.map Prod.snd := by
  -- It is the commands built from the objects.
  unfold request at h
  split at h
  · simp at h
  · split at h
    · simp at h
    · rename_i built hb
      split at h
      · simp at h
        subst h
        exact commands_objects parse objects built hb
      · simp at h

/-- A built request is within the byte bound. -/
theorem request_bytes (parse : String → Option String) (size : List Cmd → Nat)
    (maxBytes : Nat) (objects : List Obj) (cmds : List Cmd)
    (h : request parse size maxBytes objects = some cmds) :
    size cmds ≤ maxBytes := by
  -- Only a request within the bound is kept.
  unfold request at h
  split at h
  · simp at h
  · split at h
    · simp at h
    · split at h
      · rename_i within
        simp at h
        subst h
        exact within
      · simp at h

/-- A request within every bound, whose policies all read, is built. -/
theorem request_built (parse : String → Option String) (size : List Cmd → Nat)
    (maxBytes : Nat) (objects : List Obj) (cmds : List Cmd)
    (count : 1 ≤ objects.length ∧ objects.length ≤ maxList)
    (read : commands parse objects = some cmds)
    (within : size cmds ≤ maxBytes) :
    request parse size maxBytes objects = some cmds := by
  -- Every check passes, so the commands are the request.
  unfold request
  have bounds : ¬(objects.length = 0 ∨ maxList < objects.length) := by omega
  rw [if_neg bounds, read]
  simp [within]

end VeraRegistrationList
