import JsonCrdt.Lww
import JsonCrdt.Path
/-!
# JsonCrdt: the JSON CRDT's register and path-key laws

Per-path last-writer-wins registers and the path keys they live under, for the
JSON CRDT of issue #2129. The register laws are in `JsonCrdt/Lww.lean` and
the key laws in `JsonCrdt/Path.lean`; the temporal model is
`proofs/tla/JsonCrdt.tla`.
-/
