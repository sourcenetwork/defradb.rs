//! Variable handling for GraphQL queries

use rapidhash::{HashMapExt, HashSetExt, RapidHashMap, RapidHashSet};

use graphql_parser::query::{
    Definition, Directive, Document, OperationDefinition, Selection, SelectionSet, Type, Value,
    VariableDefinition,
};
use serde_json::Value as JsonValue;

use crate::error::{QueryError, Result};

use super::values::graphql_value_to_json_no_vars;

/// Merge provided variables with default values.
///
/// Provided variables take precedence over defaults.
pub(crate) fn merge_variables(
    provided: Option<&RapidHashMap<String, JsonValue>>,
    defaults: &RapidHashMap<String, JsonValue>,
) -> RapidHashMap<String, JsonValue> {
    let mut merged = defaults.clone();
    if let Some(vars) = provided {
        for (k, v) in vars {
            merged.insert(k.clone(), v.clone());
        }
    }
    merged
}

/// Extract default values from variable definitions.
///
/// Returns a RapidHashMap of variable name -> default value for all variables
/// that have a default value defined.
pub(crate) fn extract_variable_defaults(
    var_defs: &[VariableDefinition<'_, String>],
) -> Result<RapidHashMap<String, JsonValue>> {
    let mut defaults = RapidHashMap::new();
    for var_def in var_defs {
        if let Some(default_value) = &var_def.default_value {
            // Convert the default value without variable resolution (defaults can't reference other variables)
            let json_val = graphql_value_to_json_no_vars(default_value)?;
            defaults.insert(var_def.name.clone(), json_val);
        }
    }
    Ok(defaults)
}

/// Format a GraphQL type as a string (e.g. `Int!`, `[String]`, `[Int!]!`).
fn format_type(ty: &Type<'_, String>) -> String {
    match ty {
        Type::NamedType(name) => name.clone(),
        Type::NonNullType(inner) => format!("{}!", format_type(inner)),
        Type::ListType(inner) => format!("[{}]", format_type(inner)),
    }
}

/// Validate that all required (non-null) variables have been provided.
///
/// Returns an error for the first required variable that is missing from
/// the effective variables map.
pub(crate) fn validate_required_variables(
    var_defs: &[VariableDefinition<'_, String>],
    effective_variables: &RapidHashMap<String, JsonValue>,
) -> Result<()> {
    for var_def in var_defs {
        if matches!(&var_def.var_type, Type::NonNullType(_))
            && !effective_variables.contains_key(&var_def.name)
        {
            let type_str = format_type(&var_def.var_type);
            return Err(QueryError::parse(format!(
                "Variable \"${}\" of required type \"{}\" was not provided.",
                var_def.name, type_str
            )));
        }
    }
    Ok(())
}

/// Removes every use of a nullable variable the request did not provide and
/// that has no default, as Go does.
///
/// An absent variable is not `null`: the argument or input field it fills is
/// left out, so `filter: {plot: {_like: $plot}}` without `$plot` filters on
/// nothing rather than on a null pattern. A list item has no field to leave
/// out, so it becomes `null`.
pub(crate) fn omit_unset_variables(
    doc: &mut Document<'_, String>,
    provided: Option<&RapidHashMap<String, JsonValue>>,
    operation_name: Option<&str>,
) {
    let mut unset = RapidHashSet::new();
    for def in &doc.definitions {
        let Definition::Operation(op) = def else {
            continue;
        };
        let (name, var_defs) = match op {
            OperationDefinition::Query(q) => (q.name.as_deref(), &q.variable_definitions),
            OperationDefinition::Mutation(m) => (m.name.as_deref(), &m.variable_definitions),
            OperationDefinition::Subscription(s) => (s.name.as_deref(), &s.variable_definitions),
            OperationDefinition::SelectionSet(_) => continue,
        };
        if operation_name.is_some_and(|target| name != Some(target)) {
            continue;
        }
        for var_def in var_defs {
            let is_provided = provided.is_some_and(|vars| vars.contains_key(&var_def.name));
            if !is_provided
                && var_def.default_value.is_none()
                && !matches!(var_def.var_type, Type::NonNullType(_))
            {
                unset.insert(var_def.name.clone());
            }
        }
    }
    if unset.is_empty() {
        return;
    }
    for def in &mut doc.definitions {
        match def {
            Definition::Operation(OperationDefinition::Query(q)) => {
                omit_in_selection_set(&mut q.selection_set, &unset)
            }
            Definition::Operation(OperationDefinition::Mutation(m)) => {
                omit_in_selection_set(&mut m.selection_set, &unset)
            }
            Definition::Operation(OperationDefinition::Subscription(s)) => {
                omit_in_selection_set(&mut s.selection_set, &unset)
            }
            Definition::Operation(OperationDefinition::SelectionSet(ss)) => {
                omit_in_selection_set(ss, &unset)
            }
            Definition::Fragment(frag) => omit_in_selection_set(&mut frag.selection_set, &unset),
        }
    }
}

fn omit_in_selection_set(set: &mut SelectionSet<'_, String>, unset: &RapidHashSet<String>) {
    for selection in &mut set.items {
        match selection {
            Selection::Field(field) => {
                field
                    .arguments
                    .retain_mut(|(_, value)| keep_value(value, unset));
                omit_in_directives(&mut field.directives, unset);
                omit_in_selection_set(&mut field.selection_set, unset);
            }
            Selection::FragmentSpread(spread) => omit_in_directives(&mut spread.directives, unset),
            Selection::InlineFragment(inline) => {
                omit_in_directives(&mut inline.directives, unset);
                omit_in_selection_set(&mut inline.selection_set, unset);
            }
        }
    }
}

fn omit_in_directives(directives: &mut [Directive<'_, String>], unset: &RapidHashSet<String>) {
    for directive in directives {
        directive
            .arguments
            .retain_mut(|(_, value)| keep_value(value, unset));
    }
}

/// Strips unset variables out of `value`, returning false when `value` is
/// itself one and so must be left out by its parent.
fn keep_value(value: &mut Value<'_, String>, unset: &RapidHashSet<String>) -> bool {
    match value {
        Value::Variable(name) => !unset.contains(name),
        Value::Object(fields) => {
            fields.retain(|_, field| keep_value(field, unset));
            true
        }
        Value::List(items) => {
            for item in items {
                if !keep_value(item, unset) {
                    *item = Value::Null;
                }
            }
            true
        }
        _ => true,
    }
}
