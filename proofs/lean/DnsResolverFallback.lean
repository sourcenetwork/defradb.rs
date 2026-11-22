/-!
# DnsResolverFallback: the DNS layers always have a nameserver, and what the host's file says well is kept

The story, in plain words. When a libp2p node starts, its DNS layers read the
host's resolver configuration (`/etc/resolv.conf` on unix). hickory, the
parser, rejects the whole file over one line it does not like, while Go's
`dnsReadConfig` skips that line and keeps the rest. So the node first keeps
only what hickory accepts: each line hickory accepts whole stays as it is,
and a line it rejects keeps its directive with the arguments hickory accepts
one at a time, or goes away when none is accepted. If what is left names a
nameserver, that is the configuration. If it names none (an empty file, a
container, a network still coming up), the two nameservers Go's resolver
falls back to, `127.0.0.1:53` and `[::1]:53`, are added, and every other
line kept (search domains, options) stays. The file is read as bytes, as Go
reads it, so a byte that is not UTF-8 spoils only the line it is on. A file
that cannot be opened or read at all gives those two nameservers with
default options.

What must never happen: the node is left with no nameserver (hickory then
refuses to build the resolver, and the node does not start), a line hickory
accepts is dropped or changed, or a line hickory rejects reaches it.

This file proves that the chosen configuration always has a nameserver
(`chosen_has_nameservers`), that a file hickory accepts whole and that names
a nameserver is used unchanged (`usable_read_passes_through`), that every
line hickory accepts survives into the chosen configuration
(`accepted_lines_survive`), that every kept line is one hickory accepts
(`kept_lines_accepted`), and that an unreadable file gives the fallback
(`failed_read_falls_back`). Nothing here is concurrent, so there is no TLA+
model.

Rust anchors: `crates/database/p2p/src/host/transport/resolver.rs`
`read_file`, `parse_lenient`, `usable_line`, `accepted`,
`resolver_config`, `fallback_config`, `FALLBACK_NAMESERVERS`.
-/

namespace DnsResolverFallback

-- A nameserver, written as its socket address ("127.0.0.1:53").
abbrev Nameserver := String

-- Go's `defaultNS`, the nameservers used when the host has none. The Rust
-- test `the_fallback_matches_the_lean_model` reads this line.
def defaultNS : List Nameserver := ["127.0.0.1:53", "[::1]:53"]

-- One line of the file: its directive ("nameserver", "search", "options") and its arguments.
structure Line where
  -- The first word of the line.
  directive : String
  -- The words after it.
  args : List String
deriving DecidableEq

-- The nameservers a list of lines names: the arguments of its `nameserver` lines.
def servers : List Line → List String
  -- No lines name no nameserver.
  | [] => []
  -- A `nameserver` line adds its arguments; any other line adds none.
  | l :: ls => (if l.directive = "nameserver" then l.args else []) ++ servers ls

-- The fallback nameservers as lines, as `parse_lenient` appends them.
def fallbackLines : List Line := defaultNS.map fun s => ⟨"nameserver", [s]⟩

-- `usable_line`: a line hickory accepts stays whole; otherwise its directive
-- with each argument hickory accepts alone, if hickory accepts that rebuilt line.
-- `ok` is hickory's check of one line next to a valid nameserver (`accepted`).
def usableLine (ok : Line → Bool) (l : Line) : Option Line :=
  -- Accepted whole: kept unchanged.
  if ok l then some l
  else
    -- The arguments hickory accepts one at a time.
    let args := l.args.filter fun a => ok ⟨l.directive, [a]⟩
    -- The rebuilt line, kept only when it has arguments and hickory accepts it.
    if args ≠ [] ∧ ok ⟨l.directive, args⟩ then some ⟨l.directive, args⟩ else none

-- The lines `parse_lenient` keeps.
def kept (ok : Line → Bool) (ls : List Line) : List Line := ls.filterMap (usableLine ok)

-- `parse_lenient`: the kept lines, with the fallback nameservers added when they name none.
def chooseText (ok : Line → Bool) (ls : List Line) : List Line :=
  -- No nameserver left: add Go's, keeping the other lines; otherwise the kept lines.
  if servers (kept ok ls) = [] then kept ok ls ++ fallbackLines else kept ok ls

-- The result of reading the host's file: it could not be read, or here is its text.
inductive Read where
  -- The file is missing or cannot be opened or read; bytes that are not UTF-8 are not this case.
  | failed : Read
  -- The file's lines.
  | text : List Line → Read

-- What the DNS layers use: the chosen lines, or only the fallback nameservers
-- (with default options) when the file could not be read.
def choose (ok : Line → Bool) : Read → List Line
  -- An unreadable file gives the fallback alone.
  | .failed => fallbackLines
  -- A readable file goes through `parse_lenient`.
  | .text ls => chooseText ok ls

-- The nameservers of two lists joined are the nameservers of each, joined.
theorem servers_append (a b : List Line) : servers (a ++ b) = servers a ++ servers b := by
  -- One line at a time.
  induction a with
  -- Nothing in front: both sides are `servers b`.
  | nil => simp [servers]
  -- One more line in front adds the same arguments on both sides.
  | cons l ls ih => simp [servers, ih, List.append_assoc]

-- The fallback lines name a nameserver.
theorem fallback_has_servers : servers fallbackLines ≠ [] := by
  -- They are two literal `nameserver` lines.
  simp [fallbackLines, defaultNS, servers]

-- Whatever the read gives, the chosen configuration names at least one nameserver.
theorem chosen_has_nameservers (ok : Line → Bool) (r : Read) : servers (choose ok r) ≠ [] := by
  -- Look at each kind of read.
  cases r with
  -- An unreadable file gives the fallback, which names two.
  | failed => exact fallback_has_servers
  -- A readable file: either its kept lines name one, or the fallback is added.
  | text ls =>
    simp only [choose, chooseText]
    by_cases h : servers (kept ok ls) = []
    · simp [h, servers_append, fallback_has_servers]
    · simp [h]

-- When hickory accepts every line, nothing is dropped or changed.
theorem kept_all_accepted (ok : Line → Bool) (ls : List Line) (h : ∀ l ∈ ls, ok l = true) :
    kept ok ls = ls := by
  -- `usableLine` maps each accepted line to itself.
  unfold kept
  -- One line at a time.
  induction ls with
  -- No lines keep no lines.
  | nil => rfl
  -- The first line is accepted, so it is kept; the rest follow by induction.
  | cons l ls ih =>
    have hl : ok l = true := h l (by simp)
    have hr : ∀ x ∈ ls, ok x = true := fun x hx => h x (by simp [hx])
    simp [List.filterMap_cons, usableLine, hl, ih hr]

-- A file hickory accepts whole that names a nameserver is used exactly as written.
theorem usable_read_passes_through (ok : Line → Bool) (ls : List Line)
    (hok : ∀ l ∈ ls, ok l = true) (hs : servers ls ≠ []) : choose ok (.text ls) = ls := by
  -- Nothing is dropped, and a nameserver is left, so no fallback is added.
  simp [choose, chooseText, kept_all_accepted ok ls hok, hs]

-- Every line hickory accepts is in the chosen configuration, unchanged.
theorem accepted_lines_survive (ok : Line → Bool) (ls : List Line) (l : Line)
    (hm : l ∈ ls) (hl : ok l = true) : l ∈ choose ok (.text ls) := by
  -- It is kept, because `usableLine` maps it to itself.
  have hk : l ∈ kept ok ls := List.mem_filterMap.mpr ⟨l, hm, by simp [usableLine, hl]⟩
  -- The chosen lines contain the kept ones, with or without the fallback.
  simp only [choose, chooseText]
  split <;> simp [hk]

-- Every line `usableLine` keeps is one hickory accepts.
theorem usable_line_accepted (ok : Line → Bool) (l l' : Line) (h : usableLine ok l = some l') :
    ok l' = true := by
  -- Either the line was accepted whole, or the rebuilt line was checked.
  unfold usableLine at h
  split at h
  · cases h; assumption
  · simp only at h
    split at h
    · cases h; rename_i hc; exact hc.2
    · cases h

-- Every kept line is one hickory accepts, so no rejected line reaches it.
theorem kept_lines_accepted (ok : Line → Bool) (ls : List Line) (l : Line)
    (h : l ∈ kept ok ls) : ok l = true := by
  -- A kept line came out of `usableLine`.
  obtain ⟨x, _, hx⟩ := List.mem_filterMap.mp h
  -- And `usableLine` only returns accepted lines.
  exact usable_line_accepted ok x l hx

-- An unreadable file gives only the fallback nameservers.
theorem failed_read_falls_back (ok : Line → Bool) : choose ok .failed = fallbackLines := by
  -- By the definition of `choose`.
  rfl

end DnsResolverFallback
