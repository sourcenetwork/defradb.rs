/-!
# DnsResolverFallback: the DNS layers always have a nameserver, and a usable host configuration is kept

The story, in plain words. When a libp2p node starts, its DNS layers read the
host's resolver configuration (`/etc/resolv.conf` on unix). That read can
fail (the file is missing or does not parse), or it can succeed and list no
nameservers at all (a container, a sandbox, a network still coming up). In
either case the node uses the two nameservers Go's resolver falls back to,
`127.0.0.1:53` and `[::1]:53`, with default options. A read that lists at
least one nameserver is used exactly as it was read.

What must never happen: the node is left with no nameserver (hickory then
refuses to build the resolver, and the node does not start), or a usable host
configuration is replaced or changed.

This file proves that the chosen configuration never has an empty nameserver
list (`chosen_has_nameservers`), that a read with nameservers passes through
unchanged (`usable_read_passes_through`), and that a failed or empty read
gives the fallback (`failed_read_falls_back`, `empty_read_falls_back`).
Nothing here is concurrent, so there is no TLA+ model.

Rust anchors: `crates/database/p2p/src/host/transport/resolver.rs`
`resolver_config`, `fallback_config`, `FALLBACK_NAMESERVERS`.
-/

namespace DnsResolverFallback

-- A nameserver, written as its socket address ("127.0.0.1:53").
abbrev Nameserver := String

-- A resolver configuration: its nameservers and its options, which are opaque here.
structure Config (Opts : Type) where
  -- The nameservers lookups go to, in order.
  servers : List Nameserver
  -- The resolver options, kept as read.
  opts : Opts

-- Go's `defaultNS`, the nameservers used when the host has none. The Rust
-- test `the_fallback_matches_the_lean_model` reads this line.
def defaultNS : List Nameserver := ["127.0.0.1:53", "[::1]:53"]

-- The result of reading the host's configuration: an error, or a configuration.
inductive Read (Opts : Type) where
  -- The read failed (the file is missing or does not parse).
  | failed : Read Opts
  -- The read succeeded with this configuration, which may list no nameserver.
  | read : Config Opts → Read Opts

-- `resolver_config`: a read with at least one nameserver is kept as it is;
-- anything else gives the default nameservers with the default options.
def choose {Opts : Type} (defaultOpts : Opts) : Read Opts → Config Opts
  -- A read that lists nameservers is used unchanged.
  | .read c => if c.servers = [] then ⟨defaultNS, defaultOpts⟩ else c
  -- A failed read falls back.
  | .failed => ⟨defaultNS, defaultOpts⟩

-- The default nameserver list is not empty.
theorem defaultNS_nonempty : defaultNS ≠ [] := by
  -- It is a literal list of two addresses.
  simp [defaultNS]

-- Whatever the read gives, the chosen configuration lists at least one nameserver.
theorem chosen_has_nameservers {Opts : Type} (d : Opts) (r : Read Opts) :
    (choose d r).servers ≠ [] := by
  -- Look at each kind of read.
  cases r with
  -- A failed read gives the defaults, which are not empty.
  | failed => simpa [choose] using defaultNS_nonempty
  -- A successful read: either it was empty and fell back, or it kept its own non-empty list.
  | read c =>
    by_cases h : c.servers = []
    · simpa [choose, h] using defaultNS_nonempty
    · simp [choose, h]

-- A read that lists nameservers is used exactly as it was read, options included.
theorem usable_read_passes_through {Opts : Type} (d : Opts) (c : Config Opts)
    (h : c.servers ≠ []) : choose d (.read c) = c := by
  -- The list is not empty, so `choose` returns the read.
  simp [choose, h]

-- A failed read gives the default nameservers with the default options.
theorem failed_read_falls_back {Opts : Type} (d : Opts) :
    choose d (.failed : Read Opts) = ⟨defaultNS, d⟩ := by
  -- By the definition of `choose`.
  rfl

-- A read that lists no nameserver gives the default nameservers with the default options.
theorem empty_read_falls_back {Opts : Type} (d o : Opts) :
    choose d (.read ⟨[], o⟩) = ⟨defaultNS, d⟩ := by
  -- The list is empty, so `choose` falls back.
  simp [choose]

end DnsResolverFallback
