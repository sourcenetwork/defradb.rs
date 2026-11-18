/// The actor subject of a relationship: one actor, or every actor.
#[non_exhaustive]
pub enum SubjectRef {
    Actor(String),
    AllActors,
}
