use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use defra_node::{HttpConfig, Node};
use serde::{Deserialize, Serialize};
use testo::prelude::*;

use crate::actions::{Actions, Connect};
use crate::go_node::GoNode;
use crate::test_case::State;

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
}

#[derive(Debug, thiserror::Error)]
enum GoBinaryError {
    #[error("DEFRA_GO_BINARY={path} did not run `version`: {reason}")]
    Configured { path: String, reason: String },
    #[error(
        "no runnable Go defradb binary found (tried {tried}). Set DEFRA_GO_BINARY, or build the \
         pinned commit: git clone https://github.com/sourcenetwork/defradb && cd defradb && \
         git checkout 53f0e76a3 && make build (see .github/workflows/ci.yml)"
    )]
    NotFound { tried: String },
}

pub(crate) fn network_edge() -> edge::Edge {
    edge::on_demand().expect("start network edge").clone()
}

impl ClientType {
    pub const ALL: [ClientType; 4] = [Self::InProcess, Self::RustHttp, Self::GoHttp, Self::Cli];

    /// Starts a fresh database and points `state` at it. Only the in-process
    /// client gets the node itself; the others keep it alive in `held` and
    /// reach it by its URL, so an action without their body cannot quietly
    /// run in-process.
    pub fn connect(self, state: &mut State) {
        match self {
            Self::InProcess => state.node = Some(Arc::new(start_node(None))),
            Self::RustHttp | Self::Cli => {
                if self == Self::Cli {
                    let binary = crate::rust_binary();
                    assert!(
                        binary.is_file(),
                        "DEFRA_CLIENT=cli runs {}: build it with `cargo build -p cli`, or set \
                         DEFRA_RUST_BINARY",
                        binary.display()
                    );
                }
                let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind port");
                let url = format!("http://{}", listener.local_addr().expect("local address"));
                let config = HttpConfig::with_listener(listener)
                    .expect("http config")
                    .with_dev_mode(true);
                state.held = vec![Arc::new(start_node(Some(config)))];
                state.url = Some(url);
            }
            Self::GoHttp => {
                let binary = go_binary().unwrap_or_else(|error| panic!("{error}"));
                let node = GoNode::start(&binary).unwrap_or_else(|error| panic!("{error}"));
                state.url = Some(node.api_url().to_owned());
                state.held = vec![Arc::new(node)];
            }
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::InProcess => "in-process",
            Self::RustHttp => "rust-http",
            Self::GoHttp => "go-http",
            Self::Cli => "cli",
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

    /// Connects, then runs each action as this client does.
    fn apply(&self, actions: &[ActionInstance<Actions>]) -> Vec<ActionInstance<Actions>> {
        let mut result = vec![Connect { client_type: *self }.into()];
        result.extend(actions.iter().map(|instance| match instance {
            ActionInstance::Sync(action) => ActionInstance::Sync(action.for_client(*self)),
            ActionInstance::Async(action) => ActionInstance::Async(action.for_client(*self)),
        }));
        result
    }
}

fn start_node(http: Option<HttpConfig>) -> Node {
    let builder = match http {
        Some(config) => Node::builder().with_http(config),
        None => Node::builder(),
    };
    workers::block_on(builder.build())
        .expect("run builder")
        .expect("start node")
}

/// `DEFRA_GO_BINARY`, else `~/go/bin/defradb`, else `defradb` on the PATH:
/// the first that runs `version`. The version line goes to the test log.
fn go_binary() -> Result<PathBuf, GoBinaryError> {
    if let Some(path) = std::env::var_os("DEFRA_GO_BINARY") {
        let path = PathBuf::from(path);
        return match version(&path) {
            Ok(line) => Ok(announce(path, line)),
            Err(reason) => Err(GoBinaryError::Configured {
                path: path.display().to_string(),
                reason,
            }),
        };
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

fn announce(path: PathBuf, line: String) -> PathBuf {
    eprintln!("go-http: using {} — {line}", path.display());
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
