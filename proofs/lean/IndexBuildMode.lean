/-!
An HTTP request chooses its index kind and its build mode independently.
The optional vector configuration determines kind; the background flag decides
when the server responds. In particular, asking for a background build does
not turn an ordinary index into a vector index or discard a vector config.

Rust owner: crates/surfaces/http/src/handlers/index/go_api.rs go_create_index,
and crates/surfaces/http/src/index_adapter.rs IndexAdapter::create_index.
-/
namespace IndexBuildMode

-- The presence of a vector configuration selects a vector index.
def kind (hasVector _background : Bool) : Bool := hasVector

-- The build mode never changes the requested kind.
theorem kind_independent (v a b : Bool) : kind v a = kind v b := rfl

-- An ordinary background request remains an ordinary index.
theorem background_ordered : kind false true = false := rfl

-- A vector background request retains its vector kind.
theorem background_vector : kind true true = true := rfl
end IndexBuildMode
