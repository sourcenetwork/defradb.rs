------------------------ MODULE StorageCheckpoint ------------------------
\* An operator copies a stopped store into a new directory.
\* A failed copy can leave an incomplete directory. Success requires the
\* whole captured store, and an existing directory belongs to its owner.
\* StorageArgs::checkpoint in crates/entry-points/cli/src/commands/storage.rs
\* reserves the directory and waits for Regolith to finish the copy.
EXTENDS TLC
\* Each switch lets a test break one of the two checks.
CONSTANTS RequireNew, RequireComplete
\* A directory state describes what is available to the next node.
VARIABLES dest, result, occupied
\* A call starts with a free path or with a path already in use.
Init ==
    /\ occupied \in BOOLEAN \* Either initial path situation is possible.
    /\ dest = IF occupied THEN "other" ELSE "absent" \* Record what is there.
    /\ result = "waiting" \* The command has not returned.
\* Reserving a directory gives the checkpoint job a place for its files.
Reserve ==
    /\ result = "waiting" \* Reserve only while the command is running.
    /\ dest \in {"absent", "other"} \* The job has not reserved a directory yet.
    /\ (~RequireNew \/ dest = "absent") \* Require a free path when the check is on.
    /\ dest' = "partial" \* The new directory still needs its files.
    /\ UNCHANGED <<result, occupied>> \* The call and the initial path are unchanged.
\* The storage engine finishes copying its complete captured state.
Copy ==
    /\ result = "waiting" \* Finish the copy while the command is running.
    /\ dest = "partial" \* The job already has a place to write its files.
    /\ dest' = "complete" \* All captured files are available.
    /\ UNCHANGED <<result, occupied>> \* The command still needs to return.
\* A failed copy reports failure and can leave any files it already wrote.
Fail ==
    /\ result = "waiting" \* A command can fail before it returns.
    /\ result' = "failed" \* The caller sees a failure.
    /\ UNCHANGED <<dest, occupied>> \* Any files already written stay in place.
\* The command reports success only after the job completes.
Return ==
    /\ result = "waiting" \* The command reports a result once.
    /\ (~RequireComplete \/ dest = "complete") \* Require all files before success.
    /\ result' = "success" \* The caller can open the checkpoint.
    /\ UNCHANGED <<dest, occupied>> \* Reporting success does not write any files.
\* Each step acts on the same destination and call.
Next == Reserve \/ Copy \/ Fail \/ Return
\* Nothing changes once the call returns.
Spec == Init /\ [][Next]_<<dest, result, occupied>>
\* A destination that already belonged to someone keeps its contents.
ExistingUntouched == occupied => dest = "other"
\* A caller who sees success has a complete copy.
SuccessIsComplete == result = "success" => dest = "complete"
=============================================================================
