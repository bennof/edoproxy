// Copyright (c) 2026 Benjamin Benno Falkner
// SPDX-License-Identifier: MIT

use std::path::{Path, PathBuf};

fn path() -> anyhow::Result<PathBuf> {
    Ok(dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("cannot resolve home directory"))?
        .join("Library/LaunchAgents/xyz.falkner.edoproxy.plist"))
}

pub fn enabled() -> bool {
    path().is_ok_and(|path| path.is_file())
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn plist(executable: &Path, arguments: &[String]) -> String {
    let args = std::iter::once(executable.to_string_lossy().into_owned())
        .chain(arguments.iter().cloned())
        .map(|arg| format!("<string>{}</string>", escape(&arg)))
        .collect::<String>();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>xyz.falkner.edoproxy</string>
<key>ProgramArguments</key><array>{args}</array>
<key>RunAtLoad</key><true/>
<key>LimitLoadToSessionType</key><string>Aqua</string>
</dict></plist>
"#
    )
}

pub fn set_enabled(enabled: bool, arguments: &[String]) -> anyhow::Result<()> {
    use std::io::Write;
    let path = path()?;
    if enabled {
        let executable = std::env::current_exe()?;
        anyhow::ensure!(
            !executable.starts_with("/Volumes")
                && !executable.to_string_lossy().contains("AppTranslocation"),
            "Move edoproxy to Applications and launch it there before enabling automatic startup"
        );
        let parent = path.parent().unwrap();
        std::fs::create_dir_all(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(plist(&executable, arguments).as_bytes())?;
        file.as_file().sync_all()?;
        file.persist(&path)?;
    } else {
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    // launchd reads LaunchAgents at login. Do not bootstrap another copy of the
    // currently running proxy, or bootout the process handling this menu event.
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn arguments_are_xml_escaped() {
        let text = super::plist(
            std::path::Path::new("/Applications/A&B.app/proxy"),
            &["--config".into(), "/tmp/<config>.toml".into()],
        );
        assert!(text.contains("A&amp;B.app"));
        assert!(text.contains("&lt;config&gt;.toml"));
        assert!(text.contains("<key>RunAtLoad</key><true/>"));
        assert!(!text.contains("KeepAlive"));
    }
}
