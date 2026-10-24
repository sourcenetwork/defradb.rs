/-!
A storage error carries its I/O kind, operating-system code and message through
CoreKV. GraphQL exposes a storage-full code only for a typed storage-full cause.
Text alone cannot establish that cause; a failure code does not establish whether
a transaction committed. Models corekv/io_failure.rs, regolith/transaction.rs and
query/executor.rs. This changes encoding, with no recovery or retry transition.
-/
namespace StorageFailure

/-- Only the storage-full distinction affects the new wire code. -/
inductive Kind where
  | full | other
  deriving DecidableEq, Repr

/-- The I/O facts crossing the boundary, with uninterpreted message text. -/
structure IoFailure where
  kind : Kind
  osCode : Option Nat
  message : String
  deriving DecidableEq, Repr

/-- Opaque backend messages remain distinct from typed I/O errors. -/
inductive Failure where
  | io (cause : IoFailure)
  | opaque (message : String)
  deriving DecidableEq, Repr

/-- The adapter carries the I/O facts without interpreting their message. -/
def adapt (cause : IoFailure) : Failure := .io cause

/-- The wire code depends on the typed cause, never on message spelling. -/
def code : Failure → Option String
  | .io cause => if cause.kind == .full then some "STORAGE_FULL" else none
  | .opaque _ => none

/-- Adapting an I/O error retains the complete input facts. -/
theorem preserves (cause : IoFailure) : adapt cause = .io cause := rfl

/-- Every typed disk-full error receives the storage-full code. -/
theorem full_code (os : Option Nat) (message : String) :
    code (adapt ⟨.full, os, message⟩) = some "STORAGE_FULL" := rfl

/-- Message text cannot turn another I/O kind into a disk-full classification. -/
theorem other_code (os : Option Nat) (message : String) :
    code (adapt ⟨.other, os, message⟩) = none := rfl

/-- Opaque text cannot authorize recovery, even if it resembles an OS error. -/
theorem opaque_code (message : String) : code (.opaque message) = none := rfl

/-- Inputs exercise typed and opaque causes with misleading text. -/
def cases : List (String × Failure) :=
  [ ("full", adapt ⟨.full, none, "localized error"⟩)
  , ("full", adapt ⟨.full, some 28, "write-ahead log left in an unknown state"⟩)
  , ("other", adapt ⟨.other, none, "No space left on device (os error 28)"⟩)
  , ("opaque", .opaque "No space left on device (os error 28)") ]

/-- Emit model-derived inputs and expectations for the Rust regression. -/
def emit : IO Unit := do
  for (tag, failure) in cases do
    let (os, message) := match failure with
      | .io cause => (cause.osCode.map toString |>.getD "none", cause.message)
      | .opaque message => ("none", message)
    IO.println (String.intercalate "\t" [tag, os, message, (code failure).getD "none"])

end StorageFailure

/-- Executing this file produces conformance inputs and expected wire codes. -/
def main : IO Unit := StorageFailure.emit
