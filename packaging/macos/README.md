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

Drag the app to Applications, then launch it. It loads or creates `~/Library/Application Support/edoproxy/config.toml`. File logging, LaunchAgent startup management and a monochrome menu-bar template icon remain to be implemented.
