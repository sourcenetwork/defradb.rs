use testo::prelude::*;

use crate::actions::{Actions, AddCollection};
use crate::scenario::Text;

/// Adds a secondary index to every scalar field a test schema declares.
pub struct SecondaryIndex;

impl Multiplier<Actions> for SecondaryIndex {
    const NAME: &str = "secondary-index";

    fn apply(&self, actions: &[ActionInstance<Actions>]) -> Vec<ActionInstance<Actions>> {
        actions
            .iter()
            .map(|action| match action {
                ActionInstance::Sync(Actions::AddCollection(add)) => {
                    ActionInstance::Sync(Actions::AddCollection(AddCollection {
                        sdl: Text::from(indexed_sdl(add.sdl.as_str())),
                        ..add.clone()
                    }))
                }
                action => action.clone(),
            })
            .collect()
    }
}

fn indexed_sdl(sdl: &str) -> String {
    sdl.lines()
        .map(|line| {
            let indent = line.len() - line.trim_start().len();
            let body = line.trim();
            let indexable = !body.is_empty()
                && !body.starts_with("//")
                && !body.starts_with("type ")
                && body != "}"
                && body.contains(':')
                && !body.contains("@index");
            if indexable {
                format!("{}{} @index", " ".repeat(indent), body)
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_an_index_to_every_field_once() {
        let sdl = "type T {\n  name: String\n  age: Int @index\n}\n";
        assert_eq!(
            indexed_sdl(sdl),
            "type T {\n  name: String @index\n  age: Int @index\n}"
        );
    }

    #[test]
    fn leaves_comments_and_the_type_line_alone() {
        let sdl = "type T {\n  // not a field\n  name: String\n}\n";
        assert_eq!(
            indexed_sdl(sdl),
            "type T {\n  // not a field\n  name: String @index\n}"
        );
    }
}
