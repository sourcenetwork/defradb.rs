/-!
The story: an operator points a Rust DefraDB node at a rootdir whose
`config.yaml` Go DefraDB wrote. Go writes every key lowercased, and spells
many keys differently from Rust (`net.p2pdisabled` where Rust reads
`net.p2p_disabled`). Before the file is read, the loader moves each Go key
onto its Rust key, one row of a fixed table at a time.

What can happen: the file has Go keys, Rust keys, both or neither.

What must never happen: a Rust key the operator wrote is overwritten by its
Go spelling; a file with no Go keys in it (one Rust wrote) is changed at all;
two Go keys feed the same Rust key, so one silently decides the other's value.

The config is modelled flat: each dotted path (`net.p2p_disabled`) maps to
a value or to nothing. The Rust code walks nested YAML mappings, after
lowercasing section names and their keys; a path here is that section and
key joined by a dot.

What this model covers: the table step (keys.rs KEYS through `apply`), and
nothing before or after it. Lowercasing (mod.rs `lowercase_keys`), dropping
the Go-only keys (keys.rs GO_ONLY) and the `acp.document` rewrite (acp.rs) run
outside it. Lowercasing leaves a file Rust wrote as it was, since every key
Rust writes is already lowercase; on a hand-written file with two keys that
differ only in case, the later one wins.

Planted bug: making `step` let the Go key overwrite a Rust key that is
already there breaks `step_keeps`, and with it `rust_key_wins`.

Rust owners:
  crates/entry-points/cli/src/config/go_layout/keys.rs   KEYS, apply, take, put_default
  crates/entry-points/cli/src/config/go_layout/mod.rs    apply, set_default
-/

namespace GoConfigLayout

-- A config: each dotted path has a value, or nothing.
abbrev Config (α : Type) := String → Option α

-- The config with path `k` now holding `v` (or nothing, for `none`).
def put {α : Type} (m : Config α) (k : String) (v : Option α) : Config α :=
  -- every other path keeps what it had
  fun k' => if k' = k then v else m k'

-- One row of the table: the Go key `row.1` moves onto the Rust key `row.2`.
-- With no Go key there is nothing to do; with the Rust key already there the
-- Go key is dropped; otherwise the Rust key takes the Go key's value.
def step {α : Type} (m : Config α) (row : String × String) : Config α :=
  -- look at the Go key first
  match m row.1 with
  -- no Go key: the config is untouched
  | none => m
  -- a Go key: what happens depends on the Rust key
  | some v =>
    match m row.2 with
    -- the Rust key is there: it wins, and the Go key is removed
    | some _ => put m row.1 none
    -- the Rust key is absent: it takes the Go value, and the Go key is removed
    | none => put (put m row.2 (some v)) row.1 none

-- The loader: every row of the table, in order.
def apply {α : Type} : List (String × String) → Config α → Config α
  -- no rows left: the config as it now is
  | [], m => m
  -- one row, then the rest
  | row :: rest, m => apply rest (step m row)

-- The table the loader runs (keys.rs KEYS, row for row).
def table : List (String × String) := [
  ("net.p2pdisabled", "net.p2p_disabled"),
  ("net.p2paddresses", "net.p2p_addresses"),
  ("net.pubsubenabled", "net.pubsub_enabled"),
  ("net.relay", "net.relay_enabled"),
  ("api.pubkeypath", "api.pubkey_path"),
  ("api.privkeypath", "api.privkey_path"),
  ("api.allowed-origins", "api.allowed_origins"),
  ("datastore.maxtxnretries", "datastore.max_txn_retries"),
  ("datastore.noencryption", "datastore.no_encryption"),
  ("datastore.nosearchableencryption", "datastore.no_searchable_encryption"),
  ("datastore.nosigning", "datastore.no_signing"),
  ("datastore.defaultkeytype", "datastore.default_key_type"),
  ("log.colordisabled", "log.color_disabled"),
  ("secretfile", "secret_file"),
  ("telemetry.disabled", "telemetry_disabled"),
  ("replicator.retryintervals", "replicator_retry_intervals")
]

-- One row keeps any value at a path that is not that row's Go key.
theorem step_keeps {α : Type} (m : Config α) (row : String × String) (r : String) (v : α)
    (notGo : row.1 ≠ r) (has : m r = some v) : step m row r = some v := by
  -- split on whether the Go key and the Rust key are present
  unfold step
  cases hGo : m row.1 with
  -- no Go key: nothing changed
  | none => exact has
  | some g =>
    cases hRust : m row.2 with
    -- the Rust key is there: only the Go key is removed, and it is not `r`
    | some _ => simp [put, Ne.symm notGo, has]
    -- the Rust key is absent: `r` is not the Rust key, since `r` has a value
    | none =>
      have notRust : r ≠ row.2 := by
        -- if `r` were the Rust key, the Rust key would have `v`
        intro same
        rw [same] at has
        rw [has] at hRust
        cases hRust
      simp [put, Ne.symm notGo, notRust, has]

-- A value at a path that is no row's Go key survives every row.
theorem apply_keeps {α : Type} (rows : List (String × String)) (m : Config α) (r : String)
    (v : α) (notGo : ∀ row ∈ rows, row.1 ≠ r) (has : m r = some v) :
    apply rows m r = some v := by
  -- one row at a time
  induction rows generalizing m with
  -- no rows: the config is the one given
  | nil => exact has
  -- the first row keeps the value, then the rest do
  | cons row rest ih =>
    apply ih
    · intro row' mem
      exact notGo row' (List.mem_cons_of_mem _ mem)
    · exact step_keeps m row r v (notGo row (List.mem_cons_self _ _)) has

-- A config with none of the rows' Go keys is left exactly as it was.
theorem apply_identity {α : Type} (rows : List (String × String)) (m : Config α)
    (noGo : ∀ row ∈ rows, m row.1 = none) : apply rows m = m := by
  -- one row at a time: each finds no Go key and changes nothing
  induction rows with
  -- no rows: nothing changes
  | nil => rfl
  -- the first row finds no Go key, then the rest find none either
  | cons row rest ih =>
    have none1 : m row.1 = none := noGo row (List.mem_cons_self _ _)
    show apply rest (step m row) = m
    have same : step m row = m := by
      unfold step
      rw [none1]
    rw [same]
    exact ih (fun row' mem => noGo row' (List.mem_cons_of_mem _ mem))

-- No Go key in the table is also one of its Rust keys, so moving one row's
-- Go key never removes another row's Rust key.
theorem go_keys_are_not_rust_keys :
    ∀ g ∈ table.map Prod.fst, ∀ r ∈ table.map Prod.snd, g ≠ r := by
  -- checked row against row
  decide

-- No two rows feed the same Rust key: the table is injective.
theorem table_injective : (table.map Prod.snd).Nodup := by
  -- checked pair by pair
  decide

-- No Go key appears in two rows.
theorem go_keys_distinct : (table.map Prod.fst).Nodup := by
  -- checked pair by pair
  decide

-- A Rust key the file already holds is never overwritten by its Go spelling.
theorem rust_key_wins {α : Type} (m : Config α) (r : String) (v : α)
    (isRust : r ∈ table.map Prod.snd) (has : m r = some v) :
    apply table m r = some v := by
  -- no row's Go key is `r`, since `r` is a Rust key
  apply apply_keeps table m r v _ has
  intro row mem same
  exact go_keys_are_not_rust_keys row.1 (List.mem_map_of_mem _ mem) r isRust same

-- A config with no Go key of the table comes out of the table step unchanged.
theorem rust_config_unchanged {α : Type} (m : Config α)
    (noGo : ∀ row ∈ table, m row.1 = none) : apply table m = m :=
  -- the general fact, for this table
  apply_identity table m noGo

end GoConfigLayout
