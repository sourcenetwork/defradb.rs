use serde::{Deserialize, Serialize};

/// A string argument that may name a captured value: plain text, a
/// reference to a capture, or pieces joined together.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Text {
    Plain(String),
    Ref {
        #[serde(rename = "$ref")]
        name: String,
    },
    Concat {
        #[serde(rename = "$concat")]
        parts: Vec<Text>,
    },
}

impl Default for Text {
    fn default() -> Self {
        Self::Plain(String::new())
    }
}

impl From<&str> for Text {
    fn from(text: &str) -> Self {
        Self::Plain(text.to_owned())
    }
}

impl From<String> for Text {
    fn from(text: String) -> Self {
        Self::Plain(text)
    }
}

impl Text {
    pub fn reference(name: &str) -> Self {
        Self::Ref {
            name: name.to_owned(),
        }
    }

    pub fn concat<I>(parts: I) -> Self
    where
        I: IntoIterator<Item = Text>,
    {
        Self::Concat {
            parts: parts.into_iter().collect(),
        }
    }

    /// The text of a plain value. References are resolved before an op runs,
    /// so an op only ever sees plain text.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Plain(text) => text,
            _ => "",
        }
    }
}
