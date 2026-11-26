---- MODULE JsonPathMarker ----
\* Models first-byte classification in decode_json_ascending and
\* decode_json_descending in crates/engine/storage/src/encoding/json.rs.
\* The encoder writes array indices as zero; properties start with a UTF-8
\* leading byte. This model excludes UTF-8 validation and later bytes.
EXTENDS Naturals
\* Select the correct zero marker or the old, overly broad marker range.
CONSTANTS Mode
\* Only these two classification rules are modeled.
ASSUME Mode \in {"zero", "range"}
\* Record the first byte and the decoder's classification.
VARIABLES byte, isIndex
\* Group the state for the temporal specification.
vars == <<byte, isIndex>>
\* A valid UTF-8 leading byte is ASCII or a legal multibyte lead.
ValidLead(b) == b <= 127 \/ (194 <= b /\ b <= 244)
\* Classify an index by its zero marker, or by the old integer range.
Classify(b) == IF Mode = "zero" THEN b = 136 ELSE 128 <= b /\ b <= 253
\* Start with representative property leads and the array index marker.
Init == /\ byte \in {0, 65, 136, 194, 195, 233, 244}
        /\ isIndex = Classify(byte)
\* Classification is a single observation and does not change afterward.
Next == UNCHANGED vars
\* Allow the initial classification to stutter forever.
Spec == Init /\ [][Next]_vars
\* State values retain their expected types.
TypeOK == /\ byte \in Nat /\ isIndex \in BOOLEAN
\* A valid property lead must never be classified as an array index.
PropertyIsNotIndex == ValidLead(byte) => ~isIndex
\* The encoded zero array index must be classified as an index.
IndexIsIndex == byte = 136 => isIndex
====
