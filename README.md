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

The development scaffold includes the HTTP proxy, origin validation, preflight handling, a basic tray with a Quit command, integration tests, and CI for Linux, Windows, and macOS. Packaging templates are included; installable MSIX and DMG releases are not yet available.

## Quick start

Install stable Rust and Cargo. Windows requires the MSVC toolchain and Visual Studio C++ Build Tools; macOS requires Xcode Command Line Tools. Linux does not require GTK or tray libraries.

```sh
cargo run -- --origin https://spa.example.org
```

Replace the example with your SPA's exact origin, including the scheme and any non-default port, without a trailing slash.

| Option | Environment variable | Default |
| --- | --- | --- |
| `--origin` | `EDOPROXY_ORIGIN` | Required |
| `--port` | `EDOPROXY_PORT` | `8765` |
| `--headless` | — | No tray on Linux; tray on Windows/macOS |

The listener binds exclusively to `127.0.0.1`. Logs are written to stdout/stderr; set `RUST_LOG=edoproxy=debug` for debug logging. On desktop platforms, use `--headless` to run without the tray.

## Local API

The SPA checks health, sets its target server, and then sends requests through `/proxy`.

| Endpoint | Purpose |
| --- | --- |
| `GET /health` | Reports local process health, version, and whether a target is configured; does not contact the target |
| `POST /target` | Sets the target HTTP(S) origin in memory |
| `/proxy/*` | Forwards the request to the configured target, removing the `/proxy` prefix |
| `OPTIONS` | Handles CORS preflight locally |

```sh
curl http://127.0.0.1:8765/health

curl -X POST http://127.0.0.1:8765/target \
  -H 'Origin: https://spa.example.org' \
  -H 'Content-Type: application/json' \
  -d '{"url":"https://svws.example.local:8443"}'

curl http://127.0.0.1:8765/proxy/api/schema \
  -H 'Origin: https://spa.example.org'
```

The final request is forwarded to `https://svws.example.local:8443/api/schema`. The target must be an HTTP(S) origin without credentials, a path other than `/`, query parameters, or a fragment. It is never persisted and must be set again after a restart.

Only health checks accept requests without an Origin header. Other origins are rejected before handlers execute, including for requests that change state. Responses to the allowed origin receive CORS headers, including errors and preflight responses. This protects against unrelated websites; it does not authenticate local programs.

The proxy forwards the method, body, query parameters, and end-to-end headers, including `Authorization`. It removes hop-by-hop headers and the local `Origin` and `Referer`. Responses are streamed, and upstream CORS headers are replaced with the local policy. TLS certificates are verified and redirects are not followed automatically.

Browser cookie authentication using `credentials: include`, WebSockets, and redirect URL rewriting are not yet supported.

## Build and verify

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
```

The CI runs these checks on Linux, Windows, and macOS, except for the separate release build. Checkout uses a Node 24 action runtime; the application itself requires no Node.js runtime.

The server uses Tokio, axum, and reqwest. Desktop dependencies (`tray-icon`, its re-exported `muda` menu API, `tao`, and `image`) are included only on Windows/macOS.

## GitHub releases and test builds

Regular commits and pull requests run CI checks; they do not publish releases. Pushing a version tag starts the release workflow:

| Tag | GitHub release | Latest |
| --- | --- | --- |
| `v0.1.0-alpha.1` | Prerelease | No |
| `v0.1.0-beta.1` | Prerelease | No |
| `v0.1.0-rc.1` | Prerelease | No |
| `v0.1.0` | Regular release | Yes |

The tag must match the version in `Cargo.toml`, prefixed with `v`. Commit the version and updated `Cargo.lock` first, then create and push the tag:

```sh
git tag v0.1.0-alpha.1
git push origin v0.1.0-alpha.1
```

A local tag or a version change alone does not trigger publication. The workflow validates the tag, runs formatting, Clippy and tests, builds release binaries, and publishes only after all builds succeed. It attaches archives for Linux x86_64, Windows x86_64 and macOS ARM64, including the MIT license. These are standalone binaries, not MSIX/DMG installers. Each stable tag explicitly becomes Latest, including if an older stable version is tagged later. Prereleases leave the existing Latest release unchanged.

### Manual test builds

Open **Actions → Release → Run workflow**, select a branch or tag, and start the workflow. Manual runs execute the same checks and release builds, but never create or update a GitHub release, even when a tag is selected. Download the platform archives from the run's **Artifacts** section. Archive names include `test-<run-id>-<attempt>` so branch names do not affect filenames.

The workflow must be present on the repository's default branch for the Run workflow button to be available.

## Linux service

The supplied systemd unit is a **user service**. The process stays in the foreground; systemd manages its lifecycle.

1. Install the release binary at `/usr/local/bin/edoproxy`.
2. Create `~/.config/systemd/user` and `~/.config/edoproxy` if needed.
3. Copy `packaging/linux/edoproxy.service` to `~/.config/systemd/user/edoproxy.service`.
4. Copy `packaging/linux/environment.example` to `~/.config/edoproxy/environment` and set the SPA origin.
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
| macOS | DMG containing a native `.app`, initially unsigned and unnotarized, outside the App Store | [macOS packaging](packaging/macos/README.md) |
| Linux | Standalone binary managed by systemd | [User service](packaging/linux/edoproxy.service) |

Remaining implementation work:

- Persistent platform-specific configuration for the allowed origin and port, plus file logging. The target address will remain in memory.
- On Windows, resolve the MSIX package's local data directory through Windows APIs and use a `logs` subdirectory. Keep writable data separate from installed program files.
- On macOS, use `~/Library/Application Support/edoproxy` for configuration and `~/Library/Logs/edoproxy` for logs.
- Add the current target and an automatic-start checkbox to the tray. Windows will use the manifest's per-user StartupTask and respect the OS-managed state; macOS will use a per-user LaunchAgent.
- Create native application/store icons and a monochrome macOS menu-bar template icon.
- Automate MSIX and DMG builds, define the Store identity, publisher and bundle ID, and verify Store signing and submission requirements before publication.

Desktop launches currently require `--origin` or `EDOPROXY_ORIGIN`. Finder/Start menu launches need the planned configuration loading. Packaging templates are not validated installation packages.

## Artwork

- [Default logo](assets/icons/edoproxy_logo.png): text-free source for application and tray icons; embedded in the desktop binary and resized for the tray.
- [Promotional logo](assets/promotion/edoproxy_logo_text.png): includes text and is used only for promotion and this README.

## License

Copyright (c) 2026 Benjamin Benno Falkner. Licensed under the [MIT License](LICENSE).
