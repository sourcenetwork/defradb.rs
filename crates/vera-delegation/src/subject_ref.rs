/// The actor subject of a relationship: one actor, or every actor.
pub enum SubjectRef {
    Actor(String),
    AllActors,
}
