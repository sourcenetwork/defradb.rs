/-!
A browser asks for collection descriptions. An index can have stored parameters
for several algorithms. The response copies only the block for its chosen
algorithm. Integer defaults travel as JSON numbers and the browser reads them
using the same number parser as it uses for Go's response.

Rust anchors: `crates/surfaces/wasm/src/collection_js.rs`, `IndexJs::from`
and `collections_to_js`.
-/

namespace CollectionWire

-- A block is copied only when it belongs to the chosen algorithm.
def projectBlock (selected belongsTo : String) (block : Option α) : Option α :=
  if selected = belongsTo then block else none

-- A selected block is preserved, including when none was stored.
theorem selected_kept (selected : String) (block : Option α) :
    projectBlock selected selected block = block := by
  -- The algorithm matches its own block.
  simp [projectBlock]

-- A block for any other algorithm is always absent.
theorem unselected_absent (selected belongsTo : String) (block : Option α)
    (different : selected ≠ belongsTo) : projectBlock selected belongsTo block = none := by
  -- The algorithm check rules this block out.
  simp [projectBlock, different]

-- The JSON integer token is passed straight to the browser parser.
def readDefault (parseNumber : String → α) (integerToken : String) : α :=
  parseNumber integerToken

-- Go and Rust give the same browser value when they pass the same integer token.
theorem same_integer_token (parseNumber : String → α) (goToken rustToken : String)
    (same : rustToken = goToken) : readDefault parseNumber rustToken = parseNumber goToken := by
  -- Both responses use the browser's parser on the same decimal digits.
  simp [readDefault, same]

end CollectionWire
