-- A replacement chunk keeps the source, text, and hash that identify its content.
-- The replacement is built in crates/database/db/src/definition/patch/embedding_switch.rs.
namespace ChunkEmbeddingSwitch

-- Represent the unchanged chunk content independently of its vector.
structure Chunk where
  source : String
  text : String
  hash : String
  vector : List Int

-- Only replace the vector.
def replaceVector (chunk : Chunk) (vector : List Int) : Chunk :=
  { chunk with vector := vector }

-- The replacement keeps the source, text and hash and stores the supplied vector.
theorem replacement_preserves_content (chunk : Chunk) (vector : List Int) :
    let replacement := replaceVector chunk vector
    replacement.source = chunk.source ∧ replacement.text = chunk.text ∧
      replacement.hash = chunk.hash ∧ replacement.vector = vector := by
  exact ⟨rfl, rfl, rfl, rfl⟩

-- A block carries an optional commitment to the active embedding settings.
def carryCommitment (commitment : Option String) : List String :=
  commitment.toList

-- Reading the optional commitment recovers what the writer supplied.
theorem read_commitment (commitment : Option String) :
    (carryCommitment commitment).head? = commitment := by
  cases commitment <;> rfl

end ChunkEmbeddingSwitch
