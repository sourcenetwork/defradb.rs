import TextCrdt.Fugue
import TextCrdt.Span
/-!
# TextCrdt: the Text CRDT's tree laws

Fugue placement, sibling order by ID, and tombstones, for the Text CRDT of
issue #2127. The laws live in `TextCrdt/Fugue.lean`; `TextCrdt/Span.lean`
shows a run of placeholder characters kept as one node, and cut where an edit
lands, is the same tree. The temporal model is `proofs/tla/TextCrdt.tla`.
-/
