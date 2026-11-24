//! Where a filter's conditions came from, so validation can tell what the
//! request wrote from what a variable supplied.

/// One step into a filter value: an object key or a list index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathStep {
    /// An object key.
    Key(String),
    /// A list position, from 0.
    Index(usize),
}

/// A variable a literal filter names below its root, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariablePosition {
    /// The steps from the filter's root to the variable.
    pub path: Vec<PathStep>,
    /// The variable's name, without `$`.
    pub name: String,
}

/// How the request wrote a filter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum FilterSource {
    /// Written in the request, with every variable it names below its root.
    /// A filter built in code is a literal that names none.
    #[default]
    Literal,
    /// A literal naming variables at these positions.
    LiteralWithVariables(Vec<VariablePosition>),
    /// The whole filter is the value of this variable.
    Variable(String),
    /// The filter is the `filter` member of this variable, which fills a
    /// whole aggregate selector: `COUNT(books: $t)`.
    SelectorVariable(String),
}

impl FilterSource {
    /// A literal source that names `variables`, or plain [`FilterSource::Literal`].
    pub fn literal(variables: Vec<VariablePosition>) -> Self {
        if variables.is_empty() {
            Self::Literal
        } else {
            Self::LiteralWithVariables(variables)
        }
    }

    /// The variables a literal names below its root.
    pub fn variable_positions(&self) -> &[VariablePosition] {
        match self {
            Self::LiteralWithVariables(positions) => positions,
            _ => &[],
        }
    }
}

/// The argument a filter was written inside, when it is not the filter
/// argument itself: an aggregate's selector, `books: {filter: {...}}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnclosingArgument {
    /// The argument's name (`books`, `Author`).
    pub name: String,
    /// The argument's value as graphql-go prints it.
    pub printed: String,
}
