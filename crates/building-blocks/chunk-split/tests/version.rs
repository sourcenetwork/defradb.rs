use chunk_split::split::{split, ChunkConfig, SplitterVersion, UnknownSplitterVersion};

#[test]
fn version_one_is_the_current_rules() {
    assert_eq!(SplitterVersion::CURRENT.number(), 1);
    assert_eq!(SplitterVersion::try_from(1), Ok(SplitterVersion::V1));
}

#[test]
fn a_config_carries_the_definitions_version() {
    let config = ChunkConfig::new(1, 64, 1).unwrap();
    assert_eq!(config.version, SplitterVersion::V1);
    assert_eq!((config.max_chars, config.overlap), (64, 1));
    assert!(!split("One sentence. Another one.", &config).is_empty());
}

#[test]
fn a_version_this_build_lacks_is_refused() {
    for version in [0, 2, u32::MAX] {
        let error = ChunkConfig::new(version, 64, 0).unwrap_err();
        assert_eq!(error, UnknownSplitterVersion { version });
        let message = error.to_string();
        assert!(
            message.contains(&format!("version {version} ")),
            "{message}"
        );
        assert!(message.contains("knows 1"), "{message}");
        assert!(message.contains("Upgrade the node"), "{message}");
    }
}
