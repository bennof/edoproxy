<!-- Copyright (c) 2026 Benjamin Benno Falkner -->
<!-- SPDX-License-Identifier: MIT -->

# macOS / DMG

Build and package on macOS:

```sh
cargo build --release --locked
python3 scripts/build_dmg.py --binary target/release/edoproxy --output dist/edoproxy.dmg
```

The script creates a native `edoproxy.app`, generates its ICNS icon from the default logo, fills version metadata from Cargo.toml, includes the MIT license and example configuration, and creates a compressed DMG with an Applications shortcut. Existing output files are not overwritten.

The bundle ID defaults to `xyz.falkner.edoproxy` and can be overridden with `--bundle-id`. Numeric bundle versions omit the prerelease suffix; `EdoproxyVersion`, the volume name and the binary retain the full Cargo version.

No Apple developer certificate, Developer ID signing, notarization or App Store submission is used. The bundle receives a local ad-hoc signature for executable integrity, including on Apple Silicon. This does not establish Gatekeeper trust; downloaded builds may require explicit approval in macOS Privacy & Security settings.

The Release workflow includes an ARM64 DMG alongside the macOS binary archive. Manual runs only upload artifacts. Tag-triggered runs attach both files to the GitHub release.

Drag the app to Applications, then launch it. It loads or creates `~/Library/Application Support/edoproxy/config.toml`. The menu bar shows the version, local address, current target and a Start at login checkbox. The latter manages a per-user LaunchAgent for the next login. The app uses accessory activation (no Dock icon) and a monochrome template tray icon. File logging remains to be implemented.
