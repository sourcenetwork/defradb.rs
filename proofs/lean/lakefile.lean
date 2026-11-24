import Lake
open Lake DSL System

package defraConvergence where

/-- Every `*.lean` beside this file, except this file, is a root: a proof is
built because its file exists. Lake caches this list with the lakefile, so
build with `lake build -R`. -/
def proofRoots (dir : FilePath) : IO (Array Lean.Name) := do
  let entries ← dir.readDir
  let stems := entries.filterMap fun entry =>
    let file : FilePath := entry.fileName
    if file.extension == some "lean" && entry.fileName != "lakefile.lean" then file.fileStem else none
  return (stems.qsort (· < ·)).map Lean.Name.mkSimple

@[default_target]
lean_lib Proofs where
  roots := run_io proofRoots __dir__
