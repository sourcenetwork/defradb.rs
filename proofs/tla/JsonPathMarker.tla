---- MODULE JsonPathMarker ----
\* The path encoder writes array indices as the zero marker. Properties
\* begin with a UTF-8 leading byte. This model covers only the first-byte
\* classification, not UTF-8 validation or the remainder of the encoding.
EXTENDS Naturals
CONSTANTS Mode
ASSUME Mode \in {"zero", "range"}
VARIABLES byte, isIndex
vars == <<byte, isIndex>>
ValidLead(b) == b <= 127 \/ (194 <= b /\ b <= 244)
Classify(b) == IF Mode = "zero" THEN b = 136 ELSE 128 <= b /\ b <= 253
Init == /\ byte \in {0, 65, 136, 194, 195, 233, 244}
        /\ isIndex = Classify(byte)
Next == UNCHANGED vars
Spec == Init /\ [][Next]_vars
TypeOK == /\ byte \in Nat /\ isIndex \in BOOLEAN
PropertyIsNotIndex == ValidLead(byte) => ~isIndex
IndexIsIndex == byte = 136 => isIndex
====
