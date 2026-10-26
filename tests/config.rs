// Copyright (c) 2026 Benjamin Benno Falkner
// SPDX-License-Identifier: MIT

use clap::Parser;
use edoproxy::config::{Config, Settings};

#[test]
fn parses_example_and_optional_target() {
    let settings = Settings::from_toml(include_str!("../edoproxy.example.toml")).unwrap();
    assert_eq!(settings.origins.len(), 1);
    assert_eq!(settings.port, 8765);
    assert!(settings.target.is_none());
    let settings = Settings::from_toml(
        "origins = ['https://apps.falkner.xyz']\ntarget = 'https://server.example.org:8443'",
    )
    .unwrap();
    assert_eq!(
        settings.target.as_deref(),
        Some("https://server.example.org:8443")
    );
}

#[test]
fn rejects_invalid_configuration() {
    for text in [
        "origins = []",
        "origins = ['*']",
        "origins = ['https://example.org/path']",
        "origins = ['https://example.org/']",
        "origins = ['https://example.org']\nport = 0",
        "origins = ['https://example.org']\ntarget = 'file:///etc/passwd'",
        "origins = ['https://example.org']\ntarget = 'https://user:pass@example.org'",
        "origins = ['https://example.org']\ntarget = 'https://example.org/path'",
        "origins = ['https://example.org']\nunknown = true",
    ] {
        assert!(Settings::from_toml(text).is_err(), "{text}");
    }
}

#[test]
fn explicit_file_and_cli_overrides() {
    let cli = Config::try_parse_from([
        "edoproxy",
        "--config",
        "edoproxy.example.toml",
        "--origin",
        "https://override.example.org",
        "--port",
        "9000",
    ])
    .unwrap();
    let settings = cli.load().unwrap();
    assert_eq!(settings.origins, ["https://override.example.org"]);
    assert_eq!(settings.port, 9000);
}

#[test]
fn creates_missing_file_and_preserves_existing_values() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("nested/config.toml");
    let cli = Config {
        config: Some(path.clone()),
        origin: vec![],
        port: None,
        headless: true,
    };
    let settings = cli.load().unwrap();
    assert_eq!(settings.origins, ["https://apps.falkner.xyz"]);
    assert_eq!(settings.port, 8765);
    assert!(settings.target.is_none());
    let custom = "origins = ['https://custom.example.org']\nport = 9001\ntarget = 'https://server.example.org'";
    std::fs::write(&path, custom).unwrap();
    assert_eq!(cli.load().unwrap().port, 9001);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), custom);
    std::fs::write(&path, "invalid toml").unwrap();
    assert!(cli.load().is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "invalid toml");
}

#[test]
fn default_path_is_absolute_and_named() {
    let path = edoproxy::config::default_config_path().unwrap();
    assert!(path.is_absolute());
    assert_eq!(path.file_name().unwrap(), "config.toml");
}
