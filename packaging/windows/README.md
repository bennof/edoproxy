<!-- Copyright (c) 2026 Benjamin Benno Falkner -->
<!-- SPDX-License-Identifier: MIT -->

# Windows / MSIX

`AppxManifest.xml.in` is a packaging template, not a publishable package. Replace `@PACKAGE_NAME@`, `@PUBLISHER@`, and `@PUBLISHER_DISPLAY_NAME@` with the Store identity values. Set `@ARCH@` to the target architecture, such as `x64` or `arm64`.

Before the first package build:

1. Build the native release binary with `cargo build --release --locked`.
2. Implement configuration loading from the MSIX package's local data directory. The binary currently requires `--origin` or `EDOPROXY_ORIGIN`; Start menu launch is not yet configured.
3. Generate the referenced assets from `assets/icons/edoproxy_logo.png`: StoreLogo (50×50), Square150x150Logo (150×150), and Square44x44Logo (44×44).
4. Replace the manifest placeholders and stage the manifest, binary, and assets together.
5. Package with the Windows SDK's `MakeAppx`, then sign and submit according to the Store requirements.

The StartupTask is initially disabled. Tray checkbox integration and file logging remain to be implemented. Resolve writable package data paths through Windows APIs rather than hard-coding user directories.

Reference: [Windows desktop packaging extensions](https://learn.microsoft.com/en-us/windows/uwp/porting/desktop-to-uwp-extensions).
