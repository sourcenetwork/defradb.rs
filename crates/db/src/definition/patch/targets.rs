use super::*;
use serde_json::Value;

/// A patch's operations grouped by the collection version each one's path
/// names (the path's first segment, a name or a version ID, which `version_of`
/// resolves), in the order the versions first appear. Each group is named by
/// the first segment that reached it and keeps its operations in patch order.
pub(crate) fn group_by_target<F>(ops: &Value, version_of: F) -> Result<Vec<(String, Vec<Value>)>>
where
    F: Fn(&str) -> String,
{
    let invalid = |why: &str| Error::InvalidPatch(why.to_owned());
    let ops = ops
        .as_array()
        .ok_or_else(|| invalid("patch must be a JSON array"))?;
    let first = ops.first().ok_or_else(|| invalid("patch array is empty"))?;
    let first_target = target_of(first)
        .ok_or_else(|| invalid("first patch op missing 'path' field"))?
        .ok_or_else(|| invalid("patch path has no collection name"))?;
    let mut groups: Vec<(String, String, Vec<Value>)> =
        vec![(version_of(&first_target), first_target, Vec::new())];
    for op in ops {
        // An operation without a usable path stays with the first collection,
        // whose patch reports it.
        let (version, target) = match target_of(op) {
            Some(Some(target)) => (version_of(&target), target),
            _ => (groups[0].0.clone(), groups[0].1.clone()),
        };
        match groups.iter_mut().find(|(known, _, _)| *known == version) {
            Some((_, _, group)) => group.push(op.clone()),
            None => groups.push((version, target, vec![op.clone()])),
        }
    }
    Ok(groups
        .into_iter()
        .map(|(_, name, group)| (name, group))
        .collect())
}

/// The first segment of `op`'s path: `None` without a path, `Some(None)` when
/// the path names no collection.
fn target_of(op: &Value) -> Option<Option<String>> {
    let path = op.get("path")?.as_str()?;
    Some(first_segment(path).map(str::to_owned))
}

fn first_segment(path: &str) -> Option<&str> {
    path.split('/').find(|segment| !segment.is_empty())
}

/// A whole-collection path (`/<Name>` or `/<VersionID>`): its one segment.
pub(crate) fn whole_collection(path: &str) -> Option<&str> {
    let segment = first_segment(path)?;
    (path.trim_matches('/') == segment).then_some(segment)
}

fn op_name(op: &Value) -> Option<&str> {
    op.get("op").and_then(Value::as_str)
}

fn whole_at<'a>(op: &'a Value, key: &str) -> Option<&'a str> {
    op.get(key)
        .and_then(Value::as_str)
        .and_then(whole_collection)
}

/// `/to` becomes `/from` in every path and `from` of `ops`.
fn rename_target(ops: &mut [Value], to: &str, from: &str) {
    for op in ops {
        for key in ["path", "from"] {
            let Some(path) = op.get(key).and_then(Value::as_str) else {
                continue;
            };
            if first_segment(path) == Some(to) {
                op[key] = Value::String(path.replacen(to, from, 1));
            }
        }
    }
}

/// Go keys a patch's document by version ID, so copying `/X` to a new key
/// `/Y` and then removing `/X` only moves the version: it is still `X`'s.
/// Drop such a pair and send every `/Y/...` path to `/X/...`. `versions` are
/// the versions the patch starts from.
pub(crate) fn fold_collection_copies(ops: Value, versions: &[CollectionVersion]) -> Value {
    let Value::Array(mut ops) = ops else {
        return ops;
    };
    let mut index = 0;
    while index < ops.len() {
        match copy_to_fold(&ops, index, versions) {
            Some((from, to, remove_at)) => {
                ops.remove(remove_at);
                ops.remove(index);
                rename_target(&mut ops, &to, &from);
            }
            None => index += 1,
        }
    }
    Value::Array(ops)
}

/// When `ops[index]` copies collection `X` to a key `Y` no version has, a
/// later operation removes `X`, and nothing else touches `X`: `X`, `Y` and
/// the remove's index.
fn copy_to_fold(
    ops: &[Value],
    index: usize,
    versions: &[CollectionVersion],
) -> Option<(String, String, usize)> {
    let op = &ops[index];
    let (Some("copy"), Some(from), Some(to)) =
        (op_name(op), whole_at(op, "from"), whole_at(op, "path"))
    else {
        return None;
    };
    let remove_at = ops
        .iter()
        .enumerate()
        .skip(index + 1)
        .find_map(|(i, later)| {
            (op_name(later) == Some("remove") && whole_at(later, "path") == Some(from)).then_some(i)
        })?;
    let touches_from = ops.iter().enumerate().any(|(i, other)| {
        i != index
            && i != remove_at
            && [other.get("path"), other.get("from")]
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .any(|path| first_segment(path) == Some(from))
    });
    if touches_from || find_target(versions, to).is_some() {
        return None;
    }
    Some((from.to_owned(), to.to_owned(), remove_at))
}
