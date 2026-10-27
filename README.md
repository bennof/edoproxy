<!-- Copyright (c) 2026 Benjamin Benno Falkner -->
<!-- SPDX-License-Identifier: MIT -->

<p align="center">
  <img src="assets/promotion/edoproxy_logo_text.png" alt="edoproxy logo" width="320">
</p>

# edoproxy

A native Rust CORS proxy that connects a browser-based SPA to a server without suitable CORS support. It runs locally, without Tauri or a WebView.

```text
Web SPA → http://127.0.0.1:8765 → edoproxy → target server
```

Windows and macOS provide a system tray. Linux runs without a graphical session and can be managed by systemd.

## Current status

The development scaffold includes the HTTP proxy, origin validation, preflight handling, a tray showing version, local address and current target, with Quit and macOS login startup controls, integration tests, and CI for Linux, Windows, and macOS. macOS DMG and Windows MSIX packaging are included. Windows test builds include a self-signed package and its public certificate.

## Quick start

Install stable Rust and Cargo. Windows requires the MSVC toolchain and Visual Studio C++ Build Tools; macOS requires Xcode Command Line Tools. Linux does not require GTK or tray libraries.

On first launch, edoproxy creates its configuration file with `https://apps.falkner.xyz` as the only allowed origin, port `8765`, and no target. Edit the generated file to configure additional origins or a startup target:

```toml
origins = ["https://apps.falkner.xyz", "http://localhost:5173"]
port = 8765
target = "https://svws.example.local:8443" # Optional startup target
```

Origins include the scheme and optional port, without a path or trailing slash. The list must contain at least one exact origin; wildcards are not supported. Omit `target` to let the SPA configure it later.

```sh
cargo run -- --config edoproxy.toml
```

Without `--config`, the application always uses the platform's per-user path, independent of the working directory:

| Platform | Default file |
| --- | --- |
| Linux | `$XDG_CONFIG_HOME/edoproxy/config.toml`, otherwise `~/.config/edoproxy/config.toml` |
| macOS | `~/Library/Application Support/edoproxy/config.toml` |
| Windows (MSIX) | `config.toml` in the package's local data folder, resolved through Windows APIs |
| Windows (unpackaged) | `%LOCALAPPDATA%\edoproxy\config.toml`, resolved through Windows APIs |

Run `cargo run` without options to use this default. `--config` or `EDOPROXY_CONFIG` selects a different file instead. Missing files and parent directories are created, including for explicit paths. Existing files are not reset on startup. Invalid or inaccessible files cause startup to fail with a path-specific error. The selected path is logged. Files are read at startup; restart after editing them. CLI/environment overrides affect the current run only and are not saved.

| Option | Environment variable | Behavior |
| --- | --- | --- |
| `--config` | `EDOPROXY_CONFIG` | Explicit TOML path |
| `--origin` | `EDOPROXY_ORIGIN` | Replaces the file's origin list; repeat the flag or use comma-separated values |
| `--port` | `EDOPROXY_PORT` | Overrides the file's port (default: `8765`) |
| `--headless` | — | Disables the tray on desktop platforms |

CLI options take precedence over environment variables, which take precedence over file values. Files must be valid even when overrides are provided. The configuration file is loaded or created even when CLI/environment overrides are supplied.

The listener binds exclusively to `127.0.0.1`. Logs are written to stdout/stderr; set `RUST_LOG=edoproxy=debug` for debug logging. Linux always runs without a tray.

## Local API

The SPA checks health, optionally changes the configured target server, and then sends requests through `/proxy`.

| Endpoint | Purpose |
| --- | --- |
| `GET /edoproxy/health` | Reports local process health, version, and whether a target is configured; does not contact the target |
| `GET /edoproxy/target` | Returns `{"url":"https://server.example.org/"}` or `{"url":null}` |
| `POST /edoproxy/target` | Sets and persists the target using `{"url":"https://server.example.org"}` |
| `GET /edoproxy/origin` | Returns `{"origins":["https://apps.falkner.xyz"]}` |
| `POST /edoproxy/origin` | Replaces and persists the list using `{"origins":["https://apps.falkner.xyz"]}` |
| `GET /edoproxy/port` | Returns configured `port`, `active_port`, and `restart_required` |
| `POST /edoproxy/port` | Saves `{"port":8765}` and returns the port status; takes effect after restart |
| `/proxy/*` | Forwards the request to the configured target, removing the `/proxy` prefix |
| `OPTIONS` | Handles CORS preflight locally |

```sh
curl http://127.0.0.1:8765/edoproxy/health

curl -X POST http://127.0.0.1:8765/edoproxy/target \
  -H 'Origin: https://spa.example.org' \
  -H 'Content-Type: application/json' \
  -d '{"url":"https://svws.example.local:8443"}'

curl http://127.0.0.1:8765/proxy/api/schema \
  -H 'Origin: https://spa.example.org'
```

The final request is forwarded to `https://svws.example.local:8443/api/schema`. The target must be an HTTP(S) origin without credentials, a path other than `/`, query parameters, or a fragment. The optional TOML target is loaded at startup. Changes through `POST /edoproxy/target` are saved atomically to the active TOML file before the in-memory target changes. Restarting restores the last saved target. A failed save returns HTTP 500 and leaves the current target unchanged. Comments, origins and port values in the file are preserved. Avoid editing the file externally while target updates are in progress. All allowed origins share this single target.

Only health checks accept requests without an Origin header. Other origins are rejected before handlers execute, including for requests that change state. Responses to the allowed origin receive CORS headers, including errors and preflight responses. This protects against unrelated websites; it does not authenticate local programs.

The proxy forwards the method, body, query parameters, and end-to-end headers, including `Authorization`. It removes hop-by-hop headers and the local `Origin` and `Referer`. Responses are streamed, and upstream CORS headers are replaced with the local policy. TLS certificates are verified and redirects are not followed automatically.

Browser cookie authentication using `credentials: include`, WebSockets, and redirect URL rewriting are not yet supported.

Management endpoints require an already allowed Origin, except for origin-less GET health checks. The old `/health` and `/target` paths have been removed. Health has no POST operation; browser preflight remains supported.

Origin changes apply immediately after saving. Removing the caller's origin prevents its subsequent requests; the save response itself remains readable. Target and origin updates return 204, while port updates return 200 with the configured and active port. All writes are serialized and atomically replace the TOML file, preserving other settings and comments. Failed writes leave runtime values unchanged. Port changes require a restart; remove any CLI/environment port override if the saved port should take effect.

## Build and verify

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
```

The CI runs these checks on Linux, Windows, and macOS, except for the separate release build. Checkout uses a Node 24 action runtime; the application itself requires no Node.js runtime.

The server uses Tokio, axum, and reqwest. Desktop dependencies (`tray-icon`, its re-exported `muda` menu API, `tao`, and `image`) are included only on Windows/macOS.


### Manual test builds

Open **Actions → Release → Run workflow**, select a branch or tag, and start the workflow. Manual runs execute the same checks and release builds, but never create or update a GitHub release, even when a tag is selected. Download the platform archives from the run's **Artifacts** section. Archive names include `test-<run-id>-<attempt>` so branch names do not affect filenames.

The workflow must be present on the repository's default branch for the Run workflow button to be available.

## Linux release compatibility

Linux release and manual test artifacts use `x86_64-unknown-linux-musl` with a statically linked C runtime. They do not depend on the build runner's glibc version and are intended for x86_64 Debian servers as well as other compatible Linux distributions. ARM64 servers need a separate build, which is not currently included.

Ubuntu runners are pinned to `ubuntu-24.04`. The release workflow installs `musl-tools`, runs Clippy and tests for the musl target, and checks the resulting executable for dynamic loader/shared-library dependencies before packaging it. Ordinary CI still checks the native Linux target as well.

To build the release target locally on Debian/Ubuntu:

```sh
sudo apt-get install musl-tools
rustup target add x86_64-unknown-linux-musl
cargo build --release --locked --target x86_64-unknown-linux-musl
```

## Linux service

The supplied systemd unit is a **user service**. The process stays in the foreground; systemd manages its lifecycle.

1. Install the release binary at `/usr/local/bin/edoproxy`.
2. Create `~/.config/systemd/user` and `~/.config/edoproxy` if needed.
3. Copy `packaging/linux/edoproxy.service` to `~/.config/systemd/user/edoproxy.service`.
4. Optionally copy `edoproxy.example.toml` to `~/.config/edoproxy/config.toml` and edit it. If missing, the service creates it with defaults on first start.
5. Start the service:

```sh
systemctl --user daemon-reload
systemctl --user enable --now edoproxy
journalctl --user -u edoproxy -f
```

Running a user service without a logged-in session requires systemd lingering. SIGINT and SIGTERM trigger graceful server shutdown.

## Packaging and remaining work

| Platform | Distribution plan | Templates |
| --- | --- | --- |
| Windows | MSIX through the Microsoft Store | [Windows packaging](packaging/windows/README.md) |
| macOS | DMG containing a native `.app`, ad-hoc signed without a developer certificate and unnotarized, outside the App Store | [macOS packaging](packaging/macos/README.md) |
| Linux | Standalone binary managed by systemd | [User service](packaging/linux/edoproxy.service) |

Remaining implementation work:

- File logging; platform-specific TOML discovery and initial file creation are implemented.
- On Windows, add a `logs` subdirectory in the resolved local data directory. Keep writable data separate from installed program files.
- On macOS, use `~/Library/Application Support/edoproxy` for configuration and `~/Library/Logs/edoproxy` for logs.
- Add Windows automatic-start controls using the manifest's per-user StartupTask. macOS already provides a Start at login checkbox backed by a per-user LaunchAgent.
- Refine platform icon artwork as needed. macOS uses a monochrome template tray icon; its application icon remains colored.
- Define the final Store identity and publisher and verify submission requirements before Store publication. MSIX builds currently use a test identity and self-signed certificate. The macOS DMG build uses bundle ID `xyz.falkner.edoproxy`.

Desktop launches automatically load or create the per-user TOML file, including when launched through Finder or the Start menu. MSIX installation must still be verified on Windows; local macOS checks cannot exercise Windows installer behavior.

## Artwork

- [Default logo](assets/icons/edoproxy_logo.png): text-free source for application and tray icons; embedded in the desktop binary and resized for the tray.
- [Promotional logo](assets/promotion/edoproxy_logo_text.png): includes text and is used only for promotion and this README.

## License

Copyright (c) 2026 Benjamin Benno Falkner. Licensed under the [MIT License](LICENSE).

### macOS menu bar and login startup

The application runs as a menu-bar accessory without a Dock icon. Its menu shows the Cargo version, local listening address and current target, which updates after API changes. The tray icon is a monochrome template so macOS controls its appearance.

After copying the app into Applications and launching it there, enable **Start at login**. This writes `~/Library/LaunchAgents/xyz.falkner.edoproxy.plist`; disabling it removes that file. Changes take effect at the next login and do not start a duplicate instance or terminate the current one. The agent preserves the active configuration path and explicit CLI origin/port overrides. Move the app first: enabling startup from a mounted DMG or a translocated app is rejected with a message in the menu. If the installed app is moved later, re-enable startup to update its path.
