---- MODULE OrderedIndexPage ----
\* An index returns rows in order. The LimitNode above the scan applies the
\* offset; the scan must start at zero and fetch offset + limit rows, capped
\* at the largest representable scan budget. A double-offset bug skips rows
\* once at the scan and again at the LimitNode.
\* Rust: crates/engine/query/src/planner/builder/index_methods.rs
\* order_index_scan_params; the ordered input is assumed, as is a stream no
\* longer than the maximum representable budget.
EXTENDS Naturals
CONSTANT DoubleOffset
VARIABLES n, offset, limit, done, delivered
\* Row numbers also represent their order in the input scan.
vars == <<n, offset, limit, done, delivered>>
\* A bounded representation of the largest possible scan budget.
MaxBudget == 4
\* Saturating addition, as used by the scan's u64 budget.
Budget(o, l) == IF o + l > MaxBudget THEN MaxBudget ELSE o + l
\* The intended page applies offset and limit exactly once.
Page(nrows, o, l) == {i \in 1..nrows : i > o /\ i <= o + l}
\* Choose inputs including short, empty and saturated scans.
Init == /\ n \in 0..MaxBudget
        /\ offset \in 0..MaxBudget
        /\ limit \in 1..MaxBudget
        /\ done = FALSE
        /\ delivered = {}
\* The old mistake starts the scan at the offset as well as applying it
\* above; a correct scan starts at zero and reserves both skipped and kept rows.
ScanAndLimit ==
    LET start == IF DoubleOffset THEN offset ELSE 0
        stop == start + Budget(offset, limit)
    IN /\ ~done
       /\ delivered' = {i \in 1..n : i > start + offset /\
                                       i <= start + offset + limit /\ i <= stop}
       /\ done' = TRUE
       /\ UNCHANGED <<n, offset, limit>>
\* Run once; stuttering models a finished read.
Next == ScanAndLimit
Spec == Init /\ [][Next]_vars
\* The ordered scan and LimitNode return precisely the requested page.
INV_Page == done => delivered = Page(n, offset, limit)
====
