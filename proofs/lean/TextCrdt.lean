import TextCrdt.Fugue
import TextCrdt.Span
import TextCrdt.Compact
/-!
# TextCrdt: the Text CRDT's tree laws

Fugue placement, sibling order by ID, and tombstones, for the Text CRDT of
issue #2127. The laws live in `TextCrdt/Fugue.lean`; `TextCrdt/Span.lean`
shows a run of placeholder characters kept as one node, and cut where an edit
lands, is the same tree; `TextCrdt/Compact.lean` shows a replay folded down to
its text at a critical block ends at the same text. The temporal model is `proofs/tla/TextCrdt.tla`.
-/
