use std::path::PathBuf;
use std::process::Command;

use serde::{Deserialize, Serialize};
use testo::prelude::*;

use rapidhash::{HashSetExt, RapidHashSet};

use crate::actions::{Actions, Connect, Parallel};

/// How a test reaches its database. The Go suite's `DEFRA_CLIENT_*` switches.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ClientType {
    /// Calls the node directly.
    InProcess,
    /// An in-process Rust server, reached over HTTP.
    RustHttp,
    /// A Go `defradb` server process, reached over HTTP.
    GoHttp,
    /// `defra client` commands, run against an in-process Rust server.
    Cli,
    /// A Go `defradb` server built from the Shinzo pin (`GO_SHINZO_COMMIT`),
    /// reached over HTTP.
    GoShinzo,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum GoBinaryError {
    #[error("{variable}={path} did not run `version`: {reason}")]
    Configured {
        variable: &'static str,
        path: String,
        reason: String,
    },
    #[error(
        "no runnable Go defradb binary found (tried {tried}). Set DEFRA_GO_BINARY, or build the \
         pinned commit: git clone https://github.com/sourcenetwork/defradb && cd defradb && \
         git checkout 53f0e76a3 && make build (see .github/workflows/ci.yml)"
    )]
    NotFound { tried: String },
    #[error(
        "a Go Shinzo node needs DEFRA_GO_SHINZO_BINARY, a Go defradb built from the Shinzo pin \
         (GO_SHINZO_COMMIT in crates/defra-version): git clone --branch shinzo-develop \
         https://github.com/sourcenetwork/defradb && cd defradb && git checkout 2a899b860 && \
         make build (see .github/workflows/ci.yml)"
    )]
    ShinzoUnset,
}

pub(crate) fn network_edge() -> edge::Edge {
    edge::on_demand().expect("start network edge").clone()
}

impl ClientType {
    pub const ALL: [ClientType; 5] = [
        Self::InProcess,
        Self::RustHttp,
        Self::GoHttp,
        Self::Cli,
        Self::GoShinzo,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::InProcess => "in-process",
            Self::RustHttp => "rust-http",
            Self::GoHttp => "go-http",
            Self::Cli => "cli",
            Self::GoShinzo => "go-shinzo",
        }
    }

    /// The names `DEFRA_CLIENT` accepts.
    pub fn names() -> String {
        Self::ALL.map(Self::name).join(", ")
    }
}

impl Multiplier<Actions> for ClientType {
    const NAME: &str = "client-type";

    fn name(&self) -> &str {
        ClientType::name(*self)
    }

    /// Connects, then runs each action as this client does. An action aimed
    /// at a Go Shinzo node of a mixed cluster runs as the go-shinzo client
    /// whatever the test's client, since that node is reachable only over HTTP.
    /// The node a name stands for is the one started last before the action,
    /// so a name a later `NewNode` reuses keeps its earlier type until then.
    fn apply(&self, actions: &[ActionInstance<Actions>]) -> Vec<ActionInstance<Actions>> {
        let mut go_shinzo = RapidHashSet::new();
        let mut result = vec![Connect { client_type: *self }.into()];
        result.extend(actions.iter().map(|instance| match instance {
            ActionInstance::Sync(action) => {
                ActionInstance::Sync(route(action, *self, &mut go_shinzo))
            }
            ActionInstance::Async(action) => {
                ActionInstance::Async(route(action, *self, &mut go_shinzo))
            }
        }));
        result
    }
}

/// `action` as the client of the node it targets runs it: go-shinzo for a
/// node that is a Go Shinzo node at this point, `client` otherwise. Each
/// child of a parallel group is routed on its own, from the nodes as they
/// were before the group: testo runs the children on clones of the state,
/// so a child's `NewNode` reaches neither its siblings nor what follows the
/// group. `go_shinzo` holds the names that are Go Shinzo nodes now; a
/// `NewNode` updates it.
fn route(action: &Actions, client: ClientType, go_shinzo: &mut RapidHashSet<String>) -> Actions {
    match action {
        Actions::NewNode(node) => {
            let name = node.common.node().to_owned();
            if node.go_shinzo {
                go_shinzo.insert(name);
            } else {
                go_shinzo.remove(&name);
            }
            action.for_client(client)
        }
        Actions::Parallel(group) => Actions::Parallel(Parallel {
            actions: group
                .actions
                .iter()
                .map(|child| route(child, client, &mut go_shinzo.clone()))
                .collect(),
            ..group.clone()
        }),
        _ => match action.target_node() {
            Some(node) if go_shinzo.contains(&node) => action.for_client(ClientType::GoShinzo),
            _ => action.for_client(client),
        },
    }
}

/// `DEFRA_GO_BINARY`, else `~/go/bin/defradb`, else `defradb` on the PATH:
/// the first that runs `version`. The version line goes to the test log.
pub(crate) fn go_binary() -> Result<PathBuf, GoBinaryError> {
    if let Some(path) = std::env::var_os("DEFRA_GO_BINARY") {
        return configured("DEFRA_GO_BINARY", PathBuf::from(path));
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let path_var = std::env::var_os("PATH").unwrap_or_default();
    let on_path = std::env::split_paths(&path_var).map(|dir| dir.join("defradb"));
    let candidates = std::iter::once(PathBuf::from(home).join("go/bin/defradb")).chain(on_path);
    let mut tried = Vec::new();
    for path in candidates {
        match version(&path) {
            Ok(line) => return Ok(announce(path, line)),
            Err(reason) => tried.push(format!("{} ({reason})", path.display())),
        }
    }
    Err(GoBinaryError::NotFound {
        tried: tried.join(", "),
    })
}

/// `DEFRA_GO_SHINZO_BINARY`, a Go `defradb` built from the Shinzo pin. No
/// fallback: a develop binary on the PATH must never stand in for it.
pub(crate) fn go_shinzo_binary() -> Result<PathBuf, GoBinaryError> {
    let path = std::env::var_os("DEFRA_GO_SHINZO_BINARY").ok_or(GoBinaryError::ShinzoUnset)?;
    configured("DEFRA_GO_SHINZO_BINARY", PathBuf::from(path))
}

fn configured(variable: &'static str, path: PathBuf) -> Result<PathBuf, GoBinaryError> {
    match version(&path) {
        Ok(line) => Ok(announce(path, line)),
        Err(reason) => Err(GoBinaryError::Configured {
            variable,
            path: path.display().to_string(),
            reason,
        }),
    }
}

fn announce(path: PathBuf, line: String) -> PathBuf {
    eprintln!("go node: using {} — {line}", path.display());
    path
}

fn version(path: &PathBuf) -> Result<String, String> {
    let output = Command::new(path)
        .arg("version")
        .output()
        .map_err(|e| e.to_string())?;
    match output.status.success() {
        true => Ok(format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .trim()
        .to_owned()),
        false => Err(format!("exit {}", output.status)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::{NewNode, Request};
    use crate::scenario::Common;

    fn on(node: &str) -> Common {
        Common {
            node: Some(node.into()),
            ..Default::default()
        }
    }

    fn request(node: &str) -> Actions {
        Request {
            request: "query { User { name } }".into(),
            common: on(node),
            ..Default::default()
        }
        .into()
    }

    fn go_shinzo(node: &str) -> Actions {
        NewNode {
            go_shinzo: true,
            common: on(node),
            ..Default::default()
        }
        .into()
    }

    fn is_http(action: &Actions) -> bool {
        matches!(action, Actions::HttpRequest(_))
    }

    /// A node replaced by a Go Shinzo node keeps its own client until then.
    #[test]
    fn actions_route_by_the_node_active_at_that_point() {
        let actions: Vec<ActionInstance<Actions>> = vec![
            request("db").into(),
            go_shinzo("db").into(),
            request("db").into(),
        ];
        let routed = ClientType::InProcess.apply(&actions);
        let routed: Vec<&Actions> = routed
            .iter()
            .map(|instance| match instance {
                ActionInstance::Sync(action) | ActionInstance::Async(action) => action,
            })
            .collect();
        assert!(!is_http(routed[1]), "before the Go node it is in-process");
        assert!(
            is_http(routed[3]),
            "after it, the Go node is reached over HTTP"
        );
    }

    /// Each child of a parallel group goes to its own node's client.
    #[test]
    fn parallel_children_route_to_their_own_nodes() {
        let parallel = Actions::Parallel(Parallel {
            actions: vec![request("go"), request("rust")],
            ..Default::default()
        });
        let actions: Vec<ActionInstance<Actions>> = vec![go_shinzo("go").into(), parallel.into()];
        let routed = ClientType::InProcess.apply(&actions);
        let (ActionInstance::Sync(Actions::Parallel(group))
        | ActionInstance::Async(Actions::Parallel(group))) = &routed[2]
        else {
            panic!("the group stays a group");
        };
        assert!(is_http(&group.actions[0]), "the Go child goes over HTTP");
        assert!(
            !is_http(&group.actions[1]),
            "the Rust child stays in-process"
        );
    }

    /// A child's NewNode reaches neither its siblings nor what follows the
    /// group: testo runs the children on clones of the state.
    #[test]
    fn a_child_node_stays_inside_its_parallel_group() {
        let parallel = Actions::Parallel(Parallel {
            actions: vec![go_shinzo("db"), request("db")],
            ..Default::default()
        });
        let actions: Vec<ActionInstance<Actions>> = vec![parallel.into(), request("db").into()];
        let routed = ClientType::InProcess.apply(&actions);
        let (ActionInstance::Sync(Actions::Parallel(group))
        | ActionInstance::Async(Actions::Parallel(group))) = &routed[1]
        else {
            panic!("the group stays a group");
        };
        assert!(
            !is_http(&group.actions[1]),
            "a sibling keeps the pre-group node"
        );
        let (ActionInstance::Sync(after) | ActionInstance::Async(after)) = &routed[2];
        assert!(!is_http(after), "the group's node does not outlive it");
    }
}
