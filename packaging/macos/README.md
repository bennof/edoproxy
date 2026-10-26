<!-- Copyright (c) 2026 Benjamin Benno Falkner -->
<!-- SPDX-License-Identifier: MIT -->

# macOS / DMG

`Info.plist.in` defines a windowless `.app` bundle. Replace `@BUNDLE_ID@` with the final bundle identifier before packaging.

Planned layout:

```text
edoproxy.app/Contents/
  Info.plist
  MacOS/edoproxy
  Resources/edoproxy.icns
```

Build the release binary on macOS with `cargo build --release --locked`. Derive the application icon from `assets/icons/edoproxy_logo.png`. Stage the finished bundle alongside a link to `/Applications`, then create the DMG:

```sh
hdiutil create -srcfolder <staging> -format UDZO dist/edoproxy.dmg
```

Configuration loading from `~/Library/Application Support/edoproxy`, file logging to `~/Library/Logs/edoproxy`, LaunchAgent startup management, a menu-bar template icon, and automated bundle/DMG builds remain to be implemented. The application currently starts from a terminal with `--origin` or `EDOPROXY_ORIGIN`; Finder launch requires the planned configuration loading.

The initial release will be distributed outside the App Store without code signing or notarization. Gatekeeper may require manual approval when opening the application.
