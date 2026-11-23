//! Filter input validation: every filter a request writes is checked against
//! the input type graphql-go generates for it, with graphql-go's error text.
//!
//! graphql-go runs two checks. Validation checks what the request wrote
//! (literals), treating each variable as valid; if any literal is bad, those
//! errors are the response. Otherwise coercion checks the variables, and the
//! first bad one is the response.

mod print;
mod shape;
mod walk;

use std::sync::Arc;

use serde_json::{Map, Value as JsonValue};

use crate::collection_provider::CollectionProvider;
use crate::error::{QueryError, Result};
use crate::mapper::{Filter, FilterSource, Mutation, PathStep, Requestable, Select};
use schema::CollectionVersion;

pub(crate) use print::go_json;
use shape::{Collections, Shape};
use walk::Skips;

/// One collection-rooted thing to check: a select or a mutation.
pub(super) enum Root<'o> {
    Select(&'o Select),
    Mutation(&'o Mutation),
}

/// Checks every filter under `root`, whose collection is `collection`
/// (`None` for `_commits`), loading the other collections only when a
/// relation needs one.
pub(super) async fn check(
    root: Root<'_>,
    collection: Option<&CollectionVersion>,
    provider: &dyn CollectionProvider,
) -> Result<()> {
    let loaded = Vec::new();
    let found = Found::run(&root, collection, &loaded);
    if !found.missed {
        return found.into_result();
    }
    let mut all = Vec::new();
    for name in provider.list_collections().await? {
        all.extend(provider.get_collection(&name).await?);
    }
    Found::run(&root, collection, &all).into_result()
}

/// The errors found, split by check: literal errors win.
#[derive(Default)]
struct Found {
    literal: Option<String>,
    variable: Option<String>,
    missed: bool,
}

impl Found {
    fn run(
        root: &Root<'_>,
        collection: Option<&CollectionVersion>,
        all: &[Arc<CollectionVersion>],
    ) -> Self {
        let collections = Collections::new(all);
        let mut found = Found::default();
        match (root, collection) {
            (Root::Select(select), _) if select.collection_name == "_commits" => {
                found.commits(&collections, select);
            }
            (Root::Select(select), Some(collection)) => {
                found.select(&collections, collection, select);
            }
            (Root::Mutation(mutation), Some(collection)) => {
                let shape = Shape::Collection(collection);
                found.filter(&collections, shape, mutation.filter.as_ref());
                found.fields(&collections, collection, &mutation.fields);
            }
            _ => {}
        }
        found.missed = collections.missed();
        found
    }

    fn into_result(self) -> Result<()> {
        match self.literal.or(self.variable) {
            Some(message) => Err(QueryError::unknown_field(message)),
            None => Ok(()),
        }
    }

    /// `_commits`, filtered by `CommitsFilterArg`, and the commits below it.
    fn commits(&mut self, collections: &Collections<'_>, select: &Select) {
        self.filter(collections, Shape::Commits, select.filter.as_ref());
        self.commit_links(collections, select);
    }

    /// The `links` and `heads` of a commit selection (`_commits` or a
    /// collection's `_version`): Go types both as `[Commit]` with a
    /// `CommitsFilterArg` filter.
    fn commit_links(&mut self, collections: &Collections<'_>, select: &Select) {
        for field in &select.fields {
            if let Requestable::Select(sub) = field {
                if matches!(sub.collection_name.as_str(), "links" | "heads") {
                    self.commits(collections, sub);
                }
            }
        }
    }

    fn select<'a>(
        &mut self,
        collections: &Collections<'a>,
        collection: &'a CollectionVersion,
        select: &Select,
    ) {
        let shape = if select.is_encrypted {
            // Go generates no `encrypted_<Collection>` field for a collection
            // without an encrypted index: the missing field is the error.
            if collection.encrypted_indexes.is_empty() {
                return;
            }
            Shape::Encrypted(collection)
        } else {
            Shape::Collection(collection)
        };
        self.filter(collections, shape, select.filter.as_ref());
        self.fields(collections, collection, &select.fields);
    }

    fn fields<'a>(
        &mut self,
        collections: &Collections<'a>,
        collection: &'a CollectionVersion,
        fields: &[Requestable],
    ) {
        for field in fields {
            match field {
                Requestable::Select(sub) if !has_filter(sub) => {}
                Requestable::Select(sub) if sub.collection_name == "GROUP" => {
                    self.select(collections, collection, sub);
                }
                Requestable::Select(sub) if sub.collection_name == "_version" => {
                    self.commit_links(collections, sub);
                }
                Requestable::Select(sub) => {
                    let related = collection
                        .field_by_name(&sub.collection_name)
                        .filter(|f| f.kind.is_relation())
                        .and_then(|f| collections.related(collection, &f.kind));
                    if let Some(related) = related {
                        self.select(collections, related, sub);
                    }
                }
                Requestable::Aggregate(aggregate) => {
                    for target in aggregate.targets.iter().filter(|t| t.filter.is_some()) {
                        let shape = target_shape(collections, collection, &target.host_name);
                        if let Some(shape) = shape {
                            self.filter(collections, shape, target.filter.as_ref());
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn filter(&mut self, collections: &Collections<'_>, shape: Shape<'_>, filter: Option<&Filter>) {
        let Some(filter) = filter else {
            return;
        };
        let conditions = filter.conditions();
        match filter.source() {
            FilterSource::Literal | FilterSource::LiteralWithVariables(_) => {
                let positions = filter.source().variable_positions();
                if self.literal.is_none() {
                    let skips = Skips::new(positions);
                    let messages = walk::literal_fields(
                        collections,
                        shape,
                        conditions,
                        &mut Vec::new(),
                        &skips,
                    );
                    if !messages.is_empty() {
                        self.literal = Some(match filter.enclosing() {
                            Some(enclosing) => {
                                argument(&enclosing.name, &enclosing.printed, in_filter(messages))
                            }
                            None => {
                                argument("filter", &print::literal(conditions, positions), messages)
                            }
                        });
                    }
                }
                for position in positions {
                    let Some((at, whole_list)) = walk::shape_at(collections, shape, &position.path)
                    else {
                        continue;
                    };
                    let Some(value) = lookup(conditions, &position.path) else {
                        continue;
                    };
                    let messages = if whole_list {
                        walk::variable_list(collections, at, value)
                    } else {
                        walk::variable(collections, at, value)
                    };
                    self.variable_error(&position.name, &go_json(value), messages);
                }
            }
            FilterSource::Variable(name) => {
                let messages = walk::variable_fields(collections, shape, conditions);
                let printed = go_json(&JsonValue::Object(conditions.clone()));
                self.variable_error(name, &printed, messages);
            }
            FilterSource::SelectorVariable(name) => {
                let messages = in_filter(walk::variable_fields(collections, shape, conditions));
                let printed = filter
                    .enclosing()
                    .map(|e| e.printed.clone())
                    .unwrap_or_default();
                self.variable_error(name, &printed, messages);
            }
        }
    }

    fn variable_error(&mut self, name: &str, printed: &str, messages: Vec<String>) {
        if self.variable.is_none() && !messages.is_empty() {
            self.variable = Some(format!(
                "Variable \"${name}\" got invalid value {printed}.\n{}",
                messages.join("\n")
            ));
        }
    }
}

/// Whether `select` or anything selected below it carries a filter: a
/// subtree without one is not walked, so its relations are never looked up.
fn has_filter(select: &Select) -> bool {
    select.filter.is_some()
        || select.fields.iter().any(|field| match field {
            Requestable::Select(sub) => has_filter(sub),
            Requestable::Aggregate(aggregate) => {
                aggregate.targets.iter().any(|t| t.filter.is_some())
            }
            _ => false,
        })
}

/// The block an aggregate target's `filter` is checked against: the host
/// collection itself (a top-level aggregate, `_group`), a relation's
/// collection, or a scalar list's element filter.
fn target_shape<'a>(
    collections: &Collections<'a>,
    collection: &'a CollectionVersion,
    host: &str,
) -> Option<Shape<'a>> {
    if host == collection.name || host == "_group" {
        return Some(Shape::Collection(collection));
    }
    let field = collection.field_by_name(host)?;
    if field.kind.is_relation() {
        return collections
            .related(collection, &field.kind)
            .map(Shape::Collection);
    }
    Some(Collections::scalar_filter(&field.kind))
}

fn argument(name: &str, printed: &str, messages: Vec<String>) -> String {
    format!(
        "Argument \"{name}\" has invalid value {printed}.\n{}",
        messages.join("\n")
    )
}

fn in_filter(messages: Vec<String>) -> Vec<String> {
    messages
        .into_iter()
        .map(|message| format!("In field \"filter\": {message}"))
        .collect()
}

fn lookup<'v>(conditions: &'v Map<String, JsonValue>, path: &[PathStep]) -> Option<&'v JsonValue> {
    let (first, rest) = path.split_first()?;
    let PathStep::Key(key) = first else {
        return None;
    };
    let mut value = conditions.get(key)?;
    for step in rest {
        value = match step {
            PathStep::Key(key) => value.get(key)?,
            PathStep::Index(index) => value.get(*index)?,
        };
    }
    Some(value)
}
