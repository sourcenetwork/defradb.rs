use rapidhash::RapidHashMap;
use serde_json::{json, Value};

struct Node {
    links: Vec<String>,
    heads: Vec<String>,
}

fn ids(value: Option<&Value>) -> Option<Vec<String>> {
    let mut ids = value?
        .as_array()?
        .iter()
        .map(|row| row.get("cid")?.as_str().map(str::to_owned))
        .collect::<Option<Vec<_>>>()?;
    ids.sort();
    Some(ids)
}

/// Normalizes `_commits` into a CID to links and heads graph and checks the
/// shape of a two-document branchable create.
pub fn branchable_create_graph(actual: &Value) -> Option<Value> {
    let rows = actual.get("_commits")?.as_array()?;
    let mut graph: RapidHashMap<String, Node> = RapidHashMap::default();
    for row in rows {
        let id = row.get("cid")?.as_str()?.to_owned();
        let node = Node {
            links: ids(row.get("links"))?,
            heads: ids(row.get("heads"))?,
        };
        if graph.insert(id, node).is_some() {
            return None;
        }
    }
    if graph.len() != 8 {
        return None;
    }
    let leaves: Vec<&String> = graph
        .iter()
        .filter(|(_, n)| n.links.is_empty() && n.heads.is_empty())
        .map(|(id, _)| id)
        .collect();
    if leaves.len() != 4 {
        return None;
    }
    let composites: Vec<&String> = graph
        .iter()
        .filter(|(_, n)| {
            n.links.len() == 2
                && n.heads.is_empty()
                && n.links.iter().all(|link| leaves.contains(&link))
        })
        .map(|(id, _)| id)
        .collect();
    if composites.len() != 2 {
        return None;
    }
    let (mut root, mut head_count) = (false, 0);
    let mut linked: Vec<&String> = Vec::new();
    for node in graph.values() {
        let [link] = node.links.as_slice() else {
            continue;
        };
        if !composites.contains(&link) {
            continue;
        }
        if !linked.contains(&link) {
            linked.push(link);
        }
        match node.heads.len() {
            0 if root => return None,
            0 => root = true,
            1 => head_count += 1,
            _ => return None,
        }
    }
    if linked.len() != 2 || !root || head_count != 1 {
        return None;
    }
    let mut out = serde_json::Map::new();
    for (id, node) in &graph {
        out.insert(
            id.clone(),
            json!({"Links": node.links, "Heads": node.heads}),
        );
    }
    Some(Value::Object(out))
}
