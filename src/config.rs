// Copyright (c) 2026 Benjamin Benno Falkner
// SPDX-License-Identifier: MIT

use clap::Parser;

#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Config {
    /// Exact browser origin permitted to use the proxy (no path).
    #[arg(long, env = "EDOPROXY_ORIGIN")]
    pub origin: String,
    #[arg(long, env = "EDOPROXY_PORT", default_value_t = 8765)]
    pub port: u16,
    /// Run without a tray, also on desktop platforms.
    #[arg(long)]
    pub headless: bool,
}

pub fn validate_origin(value: &str) -> anyhow::Result<()> {
    let url = url::Url::parse(value)?;
    anyhow::ensure!(
        matches!(url.scheme(), "http" | "https") && url.origin().ascii_serialization() == value,
        "origin must be an exact HTTP(S) origin, e.g. https://app.example.org (without trailing slash)"
    );
    Ok(())
}
