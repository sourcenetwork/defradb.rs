-- A retry registration takes a generation and reads the peer's incarnation.
-- If the batch rolls back, the single transaction uses those same values.
-- A replacement peer cannot inherit the old registration, and an older
-- registration cannot replace a newer marker.
-- Rust: scope_batch.rs Peerstore::record_prepared_steps and
-- peerstore.rs RegisterScopeMarkerAttempt::run.
namespace RetryScopeBatch

-- A registration can write only for the incarnation it read and at a
-- generation at least as high as the current marker.
def MayWrite (captured current generation marker : Nat) : Prop :=
  captured = current ∧ marker ≤ generation

-- Running the registration on its own cannot write to a replacement peer.
theorem replacementRefused (captured current generation marker : Nat)
    (replaced : captured ≠ current) : ¬ MayWrite captured current generation marker :=
  -- The first part of permission would say the two incarnations are equal.
  fun allowed => replaced allowed.1

-- Running the registration on its own cannot overwrite a newer marker.
theorem newerMarkerPreserved (captured current generation marker : Nat)
    (newer : generation < marker) : ¬ MayWrite captured current generation marker :=
  -- The second part of permission would put the marker at or below the generation.
  fun allowed => Nat.not_le_of_lt newer allowed.2

end RetryScopeBatch
