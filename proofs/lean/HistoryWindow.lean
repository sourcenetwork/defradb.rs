import HistoryWindow.Window
import HistoryWindow.FieldReclaim

/-!
# HistoryWindow: the window and field-reclaim checks

An operator prunes a collection's history. `Window` checks which composite
commits qualify under the retention depth, and `FieldReclaim` checks which field
blocks may be removed. Both modules must preserve the live heads and the blocks
that kept composites still link.
-/
