mod runner {
    use rapidhash::RapidHashSet;
    use std::path::PathBuf;
    use std::process::ExitCode;
    use std::time::{Instant, SystemTime, UNIX_EPOCH};

    use defra_node::benchmark_support::{
        benchmark_case, seed_coding_session_fixture, CodingSessionFixtureConfig,
    };
    use defra_node::{EmbeddedNode, StorageBackend};
    use storage::backends::DurabilityMode;

    type Result<T> = std::result::Result<T, BenchError>;

    #[derive(Debug, thiserror::Error)]
    enum BenchError {
        #[error(transparent)]
        Node(#[from] defra_node::Error),
        #[error(transparent)]
        Io(#[from] std::io::Error),
        #[error(transparent)]
        Json(#[from] serde_json::Error),
        #[error("data dir already exists: {} (use --reuse or remove it first)", .0.display())]
        DataDirExists(PathBuf),
        #[error("failed to resolve data dir")]
        DataDirUnresolved,
        #[error("--reuse requires --data-dir")]
        ReuseNeedsDataDir,
        #[error("{case} explain failed: {errors}")]
        ExplainFailed { case: String, errors: String },
        #[error("missing explain response data")]
        MissingExplainData,
        #[error("failed to remove temp data dir {}: {source}", path.display())]
        RemoveTempDir {
            path: PathBuf,
            source: std::io::Error,
        },
        #[error("failed to stat {}: {source}", path.display())]
        Stat {
            path: PathBuf,
            source: std::io::Error,
        },
        #[error("failed to read {}: {source}", path.display())]
        ReadDir {
            path: PathBuf,
            source: std::io::Error,
        },
        #[error(
            "no matching cases. available cases: hot_messages_cargo, hot_messages_wand, hot_actions_cargo, hot_actions_rg, medium_messages_candidate, medium_actions_bench"
        )]
        NoMatchingCases,
        #[error("unknown argument: {0}")]
        UnknownArgument(String),
        #[error("--iterations must be greater than zero")]
        ZeroIterations,
        #[error("--limit must be greater than zero")]
        ZeroLimit,
        #[error("unknown backend: {0} (expected regolith)")]
        UnknownBackend(String),
        #[error("unknown durability: {0} (expected immediate or eventual)")]
        UnknownDurability(String),
        #[error("unknown profile: {0} (expected smoke, default, or large)")]
        UnknownProfile(String),
        #[error("missing value for {0}")]
        MissingValue(String),
        #[error("invalid value for {flag}: {value}: {source}")]
        InvalidValue {
            flag: String,
            value: String,
            source: std::num::ParseIntError,
        },
    }

    const BENCH_AT_REST_ENCRYPTION_KEY: [u8; 32] = [0x42; 32];

    #[derive(Debug)]
    struct BenchOptions {
        backend: StorageBackend,
        durability: DurabilityMode,
        data_dir: Option<PathBuf>,
        keep_data: bool,
        reuse: bool,
        explain: bool,
        at_rest_encryption: bool,
        warmup: usize,
        iterations: usize,
        limit: usize,
        case_filters: Vec<String>,
        fixture: CodingSessionFixtureConfig,
    }

    impl Default for BenchOptions {
        fn default() -> Self {
            Self {
                backend: StorageBackend::Regolith,
                durability: DurabilityMode::Immediate,
                data_dir: None,
                keep_data: false,
                reuse: false,
                explain: false,
                at_rest_encryption: false,
                warmup: 2,
                iterations: 5,
                limit: 10,
                case_filters: Vec::new(),
                fixture: CodingSessionFixtureConfig::default(),
            }
        }
    }

    pub async fn run() -> ExitCode {
        match try_run().await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("{error}");
                ExitCode::FAILURE
            }
        }
    }

    async fn try_run() -> Result<()> {
        init_tracing();
        let options = parse_args()?;
        let owned_temp_dir = match (&options.data_dir, options.reuse) {
            (Some(path), false) => {
                if path.exists() {
                    return Err(BenchError::DataDirExists(path.clone()));
                }
                None
            }
            (Some(_), true) => None,
            (None, true) => return Err(BenchError::ReuseNeedsDataDir),
            (None, false) => Some(make_temp_data_dir()),
        };

        let data_dir = owned_temp_dir
            .clone()
            .or_else(|| options.data_dir.clone())
            .ok_or(BenchError::DataDirUnresolved)?;

        let mut builder = EmbeddedNode::builder()
            .data_path(&data_dir)
            .with_storage_backend(options.backend)
            .with_storage_durability(options.durability);
        if options.at_rest_encryption {
            builder = builder.with_at_rest_encryption_key(BENCH_AT_REST_ENCRYPTION_KEY);
        }
        let open_started = Instant::now();
        let node = builder.build().await?;
        let open_elapsed = open_started.elapsed();

        let seed_started = Instant::now();
        let fixture = if options.reuse {
            options.fixture.layout()
        } else {
            seed_coding_session_fixture(&node, &options.fixture).await?
        };
        let seed_elapsed = seed_started.elapsed();
        let stats = options.fixture.estimated_stats();

        println!(
            "backend={} durability={} at_rest_encryption={} data_dir={}",
            backend_name(options.backend),
            durability_name(options.durability),
            options.at_rest_encryption,
            data_dir.display()
        );
        println!(
            "open_ms={} seed_ms={} reused={}",
            open_elapsed.as_millis(),
            seed_elapsed.as_millis(),
            options.reuse
        );
        println!(
            "fixture: sessions={} messages={} actions={} estimated_payload={}B ({:.1}MiB)",
            stats.sessions,
            stats.messages,
            stats.actions,
            stats.estimated_payload_bytes,
            stats.estimated_payload_mib(),
        );
        println!(
            "shape: hot={} messages/{} actions, medium={} messages/{} actions, background={}x{} messages/{} actions",
            fixture.hot_session.message_count,
            fixture.hot_session.action_count,
            fixture.medium_session.message_count,
            fixture.medium_session.action_count,
            fixture.background_sessions.len(),
            options.fixture.background_session_messages,
            options.fixture.background_session_actions,
        );
        println!(
            "payloads: user={}B assistant={}B action={}B",
            options.fixture.user_message_bytes,
            options.fixture.assistant_message_bytes,
            options.fixture.action_command_bytes,
        );

        let mut cases = fixture.default_cases();
        for case in &mut cases {
            case.limit = options.limit;
        }
        filter_cases(&mut cases, &options.case_filters)?;

        println!(
            "cases={} warmup={} iterations={} explain={}",
            cases.len(),
            options.warmup,
            options.iterations,
            options.explain
        );

        for case in &cases {
            if options.explain {
                let explain_query = case.render_query(true);
                let explain_response = node.execute(&explain_query).await;
                if explain_response.has_errors() {
                    let errors = explain_response
                        .errors
                        .into_iter()
                        .map(|error| error.message)
                        .collect::<Vec<_>>()
                        .join("; ");
                    return Err(BenchError::ExplainFailed {
                        case: case.name.clone(),
                        errors,
                    });
                }
                let explain_json = explain_response
                    .data
                    .ok_or(BenchError::MissingExplainData)?;
                println!("explain[{}]=", case.name);
                println!("{}", serde_json::to_string_pretty(&explain_json)?);
            }

            let summary = benchmark_case(&node, case, options.warmup, options.iterations).await?;
            println!("{}", summary.render());
        }

        let disk_bytes = dir_size_bytes(&data_dir)?;
        println!(
            "disk: bytes={} mib={:.1}",
            disk_bytes,
            disk_bytes as f64 / (1024.0 * 1024.0)
        );

        if let Some(path) = owned_temp_dir {
            if options.keep_data {
                println!("kept temp data dir: {}", path.display());
            } else {
                std::fs::remove_dir_all(&path)
                    .map_err(|source| BenchError::RemoveTempDir { path, source })?;
            }
        }

        Ok(())
    }

    fn init_tracing() {
        if std::env::var_os("RUST_LOG").is_none() {
            return;
        }

        let filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
            .add_directive(
                "iroh_quinn_proto::connection=error"
                    .parse()
                    .expect("valid tracing directive"),
            )
            .add_directive(
                "noq_proto::connection=error"
                    .parse()
                    .expect("valid tracing directive"),
            );
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(true)
            .with_ansi(true)
            .try_init();
    }

    fn filter_cases(
        cases: &mut Vec<defra_node::benchmark_support::SearchQueryCase>,
        requested: &[String],
    ) -> Result<()> {
        if requested.is_empty() {
            return Ok(());
        }

        let requested = requested.iter().cloned().collect::<RapidHashSet<_>>();
        cases.retain(|case| requested.contains(&case.name));

        if cases.is_empty() {
            return Err(BenchError::NoMatchingCases);
        }

        Ok(())
    }

    fn parse_args() -> Result<BenchOptions> {
        let mut options = BenchOptions::default();
        let mut args = std::env::args().skip(1);

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--help" | "-h" => {
                    print_usage();
                    std::process::exit(0);
                }
                "--profile" => {
                    options.fixture = parse_profile(next_value(&mut args, "--profile")?)?;
                }
                "--backend" | "--store" => {
                    options.backend = parse_backend(next_value(&mut args, &arg)?)?;
                }
                "--durability" => {
                    options.durability = parse_durability(next_value(&mut args, "--durability")?)?;
                }
                "--data-dir" => {
                    options.data_dir = Some(PathBuf::from(next_value(&mut args, "--data-dir")?));
                }
                "--keep-data" => options.keep_data = true,
                "--reuse" => options.reuse = true,
                "--explain" => options.explain = true,
                "--at-rest-encryption" => options.at_rest_encryption = true,
                "--warmup" => {
                    options.warmup = parse_usize(next_value(&mut args, "--warmup")?, "--warmup")?;
                }
                "--iterations" => {
                    options.iterations =
                        parse_usize(next_value(&mut args, "--iterations")?, "--iterations")?;
                }
                "--limit" => {
                    options.limit = parse_usize(next_value(&mut args, "--limit")?, "--limit")?;
                }
                "--case" => {
                    options
                        .case_filters
                        .push(next_value(&mut args, "--case")?.to_string());
                }
                "--hot-messages" => {
                    options.fixture.hot_session_messages =
                        parse_usize(next_value(&mut args, "--hot-messages")?, "--hot-messages")?;
                }
                "--hot-actions" => {
                    options.fixture.hot_session_actions =
                        parse_usize(next_value(&mut args, "--hot-actions")?, "--hot-actions")?;
                }
                "--medium-messages" => {
                    options.fixture.medium_session_messages = parse_usize(
                        next_value(&mut args, "--medium-messages")?,
                        "--medium-messages",
                    )?;
                }
                "--medium-actions" => {
                    options.fixture.medium_session_actions = parse_usize(
                        next_value(&mut args, "--medium-actions")?,
                        "--medium-actions",
                    )?;
                }
                "--background-sessions" => {
                    options.fixture.background_sessions = parse_usize(
                        next_value(&mut args, "--background-sessions")?,
                        "--background-sessions",
                    )?;
                }
                "--background-messages" => {
                    options.fixture.background_session_messages = parse_usize(
                        next_value(&mut args, "--background-messages")?,
                        "--background-messages",
                    )?;
                }
                "--background-actions" => {
                    options.fixture.background_session_actions = parse_usize(
                        next_value(&mut args, "--background-actions")?,
                        "--background-actions",
                    )?;
                }
                "--user-bytes" => {
                    options.fixture.user_message_bytes =
                        parse_usize(next_value(&mut args, "--user-bytes")?, "--user-bytes")?;
                }
                "--assistant-bytes" => {
                    options.fixture.assistant_message_bytes = parse_usize(
                        next_value(&mut args, "--assistant-bytes")?,
                        "--assistant-bytes",
                    )?;
                }
                "--action-bytes" => {
                    options.fixture.action_command_bytes =
                        parse_usize(next_value(&mut args, "--action-bytes")?, "--action-bytes")?;
                }
                other => return Err(BenchError::UnknownArgument(other.to_owned())),
            }
        }

        if options.iterations == 0 {
            return Err(BenchError::ZeroIterations);
        }
        if options.limit == 0 {
            return Err(BenchError::ZeroLimit);
        }

        Ok(options)
    }

    fn parse_backend(value: String) -> Result<StorageBackend> {
        match value.as_str() {
            "regolith" => Ok(StorageBackend::Regolith),
            other => Err(BenchError::UnknownBackend(other.to_owned())),
        }
    }

    fn backend_name(backend: StorageBackend) -> &'static str {
        match backend {
            StorageBackend::Regolith => "regolith",
            _ => "unknown",
        }
    }

    fn parse_durability(value: String) -> Result<DurabilityMode> {
        match value.as_str() {
            "immediate" => Ok(DurabilityMode::Immediate),
            "eventual" => Ok(DurabilityMode::Eventual),
            other => Err(BenchError::UnknownDurability(other.to_owned())),
        }
    }

    fn durability_name(durability: DurabilityMode) -> &'static str {
        match durability {
            DurabilityMode::Immediate => "immediate",
            DurabilityMode::Eventual => "eventual",
            _ => "unknown",
        }
    }

    fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
        args.next()
            .ok_or_else(|| BenchError::MissingValue(flag.to_owned()))
    }

    fn parse_usize(value: String, flag: &str) -> Result<usize> {
        value
            .parse::<usize>()
            .map_err(|source| BenchError::InvalidValue {
                flag: flag.to_owned(),
                value,
                source,
            })
    }

    fn parse_profile(value: String) -> Result<CodingSessionFixtureConfig> {
        match value.as_str() {
            "default" | "dev" => Ok(CodingSessionFixtureConfig::default()),
            "smoke" => Ok(CodingSessionFixtureConfig::smoke_test()),
            "large" => Ok(CodingSessionFixtureConfig::large()),
            other => Err(BenchError::UnknownProfile(other.to_owned())),
        }
    }

    fn make_temp_data_dir() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        std::env::temp_dir().join(format!(
            "defra-coding-session-bench-{}-{}",
            std::process::id(),
            suffix
        ))
    }

    fn dir_size_bytes(path: &std::path::Path) -> Result<u64> {
        let metadata = std::fs::metadata(path).map_err(|source| BenchError::Stat {
            path: path.to_owned(),
            source,
        })?;
        if metadata.is_file() {
            return Ok(metadata.len());
        }

        let mut total = 0;
        for entry in std::fs::read_dir(path).map_err(|source| BenchError::ReadDir {
            path: path.to_owned(),
            source,
        })? {
            let entry = entry?;
            total += dir_size_bytes(&entry.path())?;
        }
        Ok(total)
    }

    fn print_usage() {
        println!(
            "coding-session-bench\n\
             \n\
             Runs a coding-session-style nested BM25 benchmark against a local persistent store.\n\
             \n\
             Usage:\n\
               cargo run -p defra-node --bin coding-session-bench -- [options]\n\
             \n\
             Options:\n\
               --profile NAME            Fixture profile: smoke, default, or large\n\
               --backend NAME            Backend: regolith (the only one)\n\
               --store NAME              Alias for --backend\n\
               --durability NAME         Durability: immediate or eventual (default: immediate)\n\
               --data-dir PATH            Persist the store at PATH\n\
               --reuse                    Reuse an existing fixture at --data-dir\n\
               --keep-data                Keep the auto-created temp data dir after the run\n\
               --at-rest-encryption       Encrypt Regolith engine files at rest\n\
               --warmup N                 Warmup iterations per case (default: 2)\n\
               --iterations N             Timed iterations per case (default: 5)\n\
               --limit N                  GraphQL limit per query (default: 10)\n\
               --case NAME                Run only a named case (repeatable)\n\
               --explain                  Print @explain(type: execute) output for each case\n\
               --hot-messages N           Hot session message count (default: 1500)\n\
               --hot-actions N            Hot session action count (default: 900)\n\
               --medium-messages N        Medium session message count (default: 500)\n\
               --medium-actions N         Medium session action count (default: 250)\n\
               --background-sessions N    Background session count (default: 12)\n\
               --background-messages N    Background session message count (default: 120)\n\
               --background-actions N     Background session action count (default: 60)\n\
               --user-bytes N             Target bytes for user messages before session scaling\n\
               --assistant-bytes N        Target bytes for assistant messages before session scaling\n\
               --action-bytes N           Target bytes for action commands before session scaling\n\
             \n\
             Cases:\n\
               hot_messages_cargo\n\
               hot_messages_wand\n\
               hot_actions_cargo\n\
               hot_actions_rg\n\
               medium_messages_candidate\n\
               medium_actions_bench"
        );
    }
}

/// The allocator the `defra` binary ships with, so this measures the node as
/// it runs.
#[cfg(not(target_arch = "wasm32"))]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> std::process::ExitCode {
    workers::block_on(runner::run()).unwrap_or(std::process::ExitCode::FAILURE)
}
