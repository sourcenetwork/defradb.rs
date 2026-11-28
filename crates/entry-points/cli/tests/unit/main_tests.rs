use super::*;

#[test]
fn profile_flag_controls_startup_profiling() {
    for (args, expected) in [
        (vec!["defra", "start"], false),
        (vec!["defra", "start", "--profile"], true),
        (vec!["defra", "version"], false),
    ] {
        let cli = Cli::try_parse_from(args).unwrap();
        assert_eq!(should_profile(&cli), expected);
    }
}

#[test]
fn the_network_runtime_follows_the_worker_placement() {
    use workers::Placement;
    let placement = |args: &[&str], config: &Config| {
        edge_placement(&Cli::try_parse_from(args).unwrap(), config)
    };
    let defaults = Config::default();
    assert_eq!(
        placement(&["defra", "start"], &defaults),
        Placement::default()
    );
    assert_eq!(
        placement(
            &["defra", "start", "--worker-placement=unpinned"],
            &defaults
        ),
        Placement::Unpinned
    );
    assert_eq!(
        placement(&["defra", "start", "--worker-placement=nowhere"], &defaults),
        Placement::default(),
        "a value the pool cannot read is start's to refuse"
    );
    let mut file = Config::default();
    file.workers.placement = Some("auto".into());
    assert_eq!(placement(&["defra", "start"], &file), Placement::Auto);
    assert_eq!(
        placement(&["defra", "start", "--worker-placement=spread"], &file),
        Placement::Spread,
        "the flag over the config file"
    );
    assert_eq!(
        placement(&["defra", "version"], &file),
        Placement::Auto,
        "a command with no worker flags keeps the config file's"
    );
}
