<!-- Copyright (c) 2026 Benjamin Benno Falkner -->
<!-- SPDX-License-Identifier: MIT -->

# Windows / MSIX

The MSIX contains the same proxy, tray, management endpoints and persistent TOML state as the macOS application. The Start menu entry launches without a console in release builds. Packaged launches create or load `config.toml` in the package-local data folder; no command-line configuration is required.

## Build on Windows

Install Rust/MSVC, Visual Studio C++ Build Tools, PowerShell 7 and the Windows SDK (MakeAppx and SignTool). Run test-signed builds in an elevated PowerShell session; GitHub-hosted Windows runners already provide the required permissions.

```powershell
$env:RUSTFLAGS = '-C target-feature=+crt-static'
cargo build --release --locked
./scripts/build_msix.ps1 -Binary target/release/edoproxy.exe -Output dist/edoproxy.msix -TestSign
```

The script fills the manifest, generates transparent logo assets, includes the license and example TOML, builds and unpacks the package for verification, and optionally signs it with a temporary self-signed certificate. Only the public `.cer` file is exported; the temporary private key is removed. For `SignTool verify /pa /v`, the build temporarily imports the public certificate into LocalMachine TrustedPeople and removes that entry in a `finally` block. It does not install a trusted root certificate.

The Release workflow uploads the x64 MSIX and its public test certificate alongside the standalone ZIP. Manual runs only produce artifacts; version tags attach them to the release. These are test-signed packages, not Microsoft Store submissions.

## Install a test build

Download the matching `.msix` and `.cer` from the same run. After verifying that you trust the source, import the certificate in an elevated PowerShell session:

```powershell
Import-Certificate -FilePath ./edoproxy.cer -CertStoreLocation Cert:\LocalMachine\TrustedPeople
```

Use the actual downloaded filenames. Then double-click the MSIX or run `Add-AppxPackage -Path ./edoproxy.msix` as the intended user. Start edoproxy from the Start menu. Each build creates a new test certificate; trust its matching certificate before installation and remove old test certificates when no longer needed. Reinstalling the same package version may require uninstalling the previous test build; back up its configuration first.

## Versions and Store distribution

The full Cargo version is retained in the binary and packaged `VERSION` file. MSIX requires four numeric components. Default test version mapping uses the Cargo major/minor/patch and revision `10000 + N` for alpha, `20000 + N` for beta, `30000 + N` for rc and `60000` for stable. For example, `0.1.0-alpha.1` becomes `0.1.0.10001`. Prerelease counters must be below 10000.

For Store submission, supply the assigned `-PackageName` and `-Publisher`, set `-PackageVersion` according to Store requirements (including its reserved fourth component), and omit `-TestSign`. Store identity and submission remain separate setup steps. The default identity is `Falkner.edoproxy`, publisher `CN=Benjamin Benno Falkner`.

The StartupTask is disabled initially. Autostart controls and file logging remain future work on Windows, just as in the macOS implementation.

Reference: [Microsoft's MSIX signing guide](https://learn.microsoft.com/en-us/windows/msix/package/sign-msix-package-guide).
