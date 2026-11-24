/-!
An execution explain replaces a child scan's counters after a join. If the
join performs a direct foreign-key index lookup, the name must identify that
lookup. Otherwise the original child scan's name remains visible.

Rust owner: crates/engine/query/src/plan/type_join/mod.rs
`scan_node_with_index_name`; type_join_one.rs `explain_execute_inner`.
-/
namespace ExplainIndexName

-- A direct lookup takes precedence over the child plan's optional name.
def selected (lookup child : Option Nat) : Option Nat := lookup.orElse fun _ => child

-- A direct lookup is always named, even when the child plan has another name.
theorem lookup_wins (name : Nat) (child : Option Nat) :
    selected (some name) child = some name := by rfl

-- Without a direct lookup, the child plan's name is preserved.
theorem child_is_fallback (child : Option Nat) : selected none child = child := by rfl

-- If neither path uses an index, no index is named.
theorem no_index_no_name : selected none none = none := by rfl
end ExplainIndexName
