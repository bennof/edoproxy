// Copyright (c) 2026 Benjamin Benno Falkner
// SPDX-License-Identifier: MIT

use clap::Parser;

#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Config {
    /// Override the platform configuration path. Missing files are created.
    #[arg(long, env = "EDOPROXY_CONFIG")]
    pub config: Option<std::path::PathBuf>,
    /// Allowed origins; replaces the file's list when supplied.
    #[arg(long, env = "EDOPROXY_ORIGIN", value_delimiter = ',')]
    pub origin: Vec<String>,
    #[arg(long, env = "EDOPROXY_PORT")]
    pub port: Option<u16>,
    /// Run without a tray, also on desktop platforms.
    #[arg(long)]
    pub headless: bool,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub origins: Vec<String>,
    #[serde(default = "default_port")]
    pub port: u16,
    pub target: Option<String>,
}

fn default_port() -> u16 {
    8765
}

impl Settings {
    pub fn from_toml(text: &str) -> anyhow::Result<Self> {
        let settings: Self = toml::from_str(text)?;
        settings.validate()?;
        Ok(settings)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.origins.is_empty(),
            "at least one allowed origin is required"
        );
        for origin in &self.origins {
            validate_origin(origin)?;
        }
        anyhow::ensure!(self.port != 0, "port must be between 1 and 65535");
        if let Some(target) = &self.target {
            validate_target(target)?;
        }
        Ok(())
    }
}

impl Config {
    pub fn path(&self) -> anyhow::Result<std::path::PathBuf> {
        match &self.config {
            Some(path) => Ok(std::path::absolute(path)?),
            None => default_config_path(),
        }
    }

    pub fn load(&self) -> anyhow::Result<Settings> {
        use anyhow::Context;
        let path = self.path()?;
        ensure_config_file(&path)?;
        tracing::info!(path = %path.display(), "loading configuration");
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("cannot read configuration: {}", path.display()))?;
        let mut settings = Settings::from_toml(&text)
            .with_context(|| format!("invalid configuration: {}", path.display()))?;
        if !self.origin.is_empty() {
            settings.origins = self.origin.clone();
        }
        if let Some(port) = self.port {
            settings.port = port;
        }
        settings.validate()?;
        Ok(settings)
    }
}

/// Resolve the configuration independently of the process working directory.
pub fn default_config_path() -> anyhow::Result<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    {
        // Packaged applications use their package-local storage.
        if let Ok(data) = windows::Storage::ApplicationData::Current() {
            let folder = data.LocalFolder()?.Path()?;
            return Ok(std::path::PathBuf::from(folder.to_string()).join("config.toml"));
        }
    }
    #[cfg(target_os = "windows")]
    let root = dirs::data_local_dir();
    #[cfg(not(target_os = "windows"))]
    let root = dirs::config_dir();
    Ok(root
        .ok_or_else(|| {
            anyhow::anyhow!("cannot resolve user configuration directory; use --config")
        })?
        .join("edoproxy")
        .join("config.toml"))
}

/// Create defaults only when absent; never replace an existing configuration.
pub fn ensure_config_file(path: &std::path::Path) -> anyhow::Result<()> {
    use anyhow::Context;
    use std::io::Write;
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "cannot create configuration directory: {}",
                parent.display()
            )
        })?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(mut file) => {
            file.write_all(include_bytes!("../edoproxy.example.toml"))
                .with_context(|| {
                    format!("cannot write default configuration: {}", path.display())
                })?;
            file.sync_all()?;
            tracing::info!(path = %path.display(), "created default configuration");
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("cannot create configuration: {}", path.display()));
        }
    }
    Ok(())
}

pub fn validate_target(value: &str) -> anyhow::Result<url::Url> {
    let url = url::Url::parse(value)?;
    anyhow::ensure!(
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && url.path() == "/",
        "target must be an HTTP(S) origin without credentials, path, query or fragment"
    );
    Ok(url)
}

pub fn validate_origin(value: &str) -> anyhow::Result<()> {
    let url = url::Url::parse(value)?;
    anyhow::ensure!(
        matches!(url.scheme(), "http" | "https") && url.origin().ascii_serialization() == value,
        "origin must be an exact HTTP(S) origin, e.g. https://app.example.org (without trailing slash)"
    );
    Ok(())
}

/// Update one persisted setting, preserving comments and other file settings.
pub fn save_value(path: &std::path::Path, key: &str, value: toml_edit::Item) -> anyhow::Result<()> {
    use anyhow::Context;
    use std::io::Write;
    let text = std::fs::read_to_string(path).context("cannot read state file")?;
    Settings::from_toml(&text)?;
    let mut document = text.parse::<toml_edit::Document>()?;
    document[key] = value;
    Settings::from_toml(&document.to_string())?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary
        .as_file()
        .set_permissions(std::fs::metadata(path)?.permissions())?;
    temporary.write_all(document.to_string().as_bytes())?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .context("cannot replace state file")?;
    Ok(())
}
