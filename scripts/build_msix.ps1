# Copyright (c) 2026 Benjamin Benno Falkner
# SPDX-License-Identifier: MIT

[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Binary,
    [Parameter(Mandatory)][string]$Output,
    [ValidateSet('x64', 'arm64')][string]$Architecture = 'x64',
    [string]$PackageName = 'Falkner.edoproxy',
    [string]$Publisher = 'CN=Benjamin Benno Falkner',
    [string]$PackageVersion,
    [switch]$TestSign
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $IsWindows) { throw 'MSIX packaging requires Windows and the Windows SDK.' }
$root = Split-Path $PSScriptRoot -Parent
$binaryPath = (Resolve-Path $Binary).Path
$outputPath = [IO.Path]::GetFullPath($Output)
if (Test-Path $outputPath) { throw "Output already exists: $outputPath" }
$certificatePath = [IO.Path]::ChangeExtension($outputPath, '.cer')
if ($TestSign -and (Test-Path $certificatePath)) { throw "Output already exists: $certificatePath" }
$sdkRoot = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin'
$sdk = Get-ChildItem $sdkRoot -Directory | Where-Object { $_.Name -match '^10\.' } |
    Sort-Object { [version]$_.Name } -Descending |
    Where-Object { (Test-Path (Join-Path $_.FullName 'x64/makeappx.exe')) -and (Test-Path (Join-Path $_.FullName 'x64/signtool.exe')) } |
    Select-Object -First 1
if (-not $sdk) { throw 'Windows SDK MakeAppx and SignTool are required.' }
$makeappx = Join-Path $sdk.FullName 'x64/makeappx.exe'
$signtool = Join-Path $sdk.FullName 'x64/signtool.exe'
$metadata = & cargo metadata --no-deps --format-version 1 --manifest-path (Join-Path $root 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw 'Cannot read Cargo metadata.' }
$package = ($metadata | ConvertFrom-Json).packages | Where-Object name -eq 'edoproxy'
$version = $package.version
if (-not $PackageVersion) {
    if ($version -notmatch '^(\d+)\.(\d+)\.(\d+)(?:-(alpha|beta|rc)\.(\d+))?$') { throw "Unsupported version: $version" }
    $revision = 60000
    if ($Matches[4]) {
        $base = @{ alpha = 10000; beta = 20000; rc = 30000 }[$Matches[4]]
        $sequence = [int]$Matches[5]
        if ($sequence -ge 10000) { throw 'Prerelease sequence must be below 10000.' }
        $revision = $base + $sequence
    }
    $PackageVersion = "$($Matches[1]).$($Matches[2]).$($Matches[3]).$revision"
}
$parsedVersion = [version]$PackageVersion
if ($parsedVersion.Revision -lt 0 -or @($parsedVersion.Major, $parsedVersion.Minor, $parsedVersion.Build, $parsedVersion.Revision).Where({ $_ -gt 65535 }).Count) {
    throw 'MSIX version must have four components between 0 and 65535.'
}
$work = Join-Path ([IO.Path]::GetTempPath()) ('edoproxy-msix-' + [guid]::NewGuid())
$stage = Join-Path $work 'stage'
$assets = Join-Path $stage 'Assets'
$certificate = $null
try {
    New-Item -ItemType Directory -Path $assets -Force | Out-Null
    New-Item -ItemType Directory -Path (Split-Path $outputPath -Parent) -Force | Out-Null
    Copy-Item $binaryPath (Join-Path $stage 'edoproxy.exe')
    foreach ($name in @('LICENSE', 'edoproxy.example.toml')) { Copy-Item (Join-Path $root $name) $stage }
    Set-Content (Join-Path $stage 'VERSION') $version
    [xml]$manifest = Get-Content (Join-Path $root 'packaging/windows/AppxManifest.xml.in') -Raw
    $manifest.Package.Identity.SetAttribute('Name', $PackageName)
    $manifest.Package.Identity.SetAttribute('Publisher', $Publisher)
    $manifest.Package.Identity.SetAttribute('Version', $PackageVersion)
    $manifest.Package.Identity.SetAttribute('ProcessorArchitecture', $Architecture)
    $manifest.Package.Properties.PublisherDisplayName = 'Benjamin Benno Falkner'
    $manifest.Save((Join-Path $stage 'AppxManifest.xml'))
    Add-Type -AssemblyName System.Drawing
    $source = [Drawing.Image]::FromFile((Join-Path $root 'assets/icons/edoproxy_logo.png'))
    try {
        foreach ($entry in @{ StoreLogo = 50; Square150x150Logo = 150; Square44x44Logo = 44 }.GetEnumerator()) {
            $size = [int]$entry.Value
            $bitmap = [Drawing.Bitmap]::new($size, $size)
            $graphics = [Drawing.Graphics]::FromImage($bitmap)
            try {
                $graphics.Clear([Drawing.Color]::Transparent)
                $graphics.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
                $scale = [Math]::Min($size / $source.Width, $size / $source.Height)
                $width = [int]($source.Width * $scale)
                $height = [int]($source.Height * $scale)
                $graphics.DrawImage($source, [int](($size - $width) / 2), [int](($size - $height) / 2), $width, $height)
                $bitmap.Save((Join-Path $assets ($entry.Key + '.png')), [Drawing.Imaging.ImageFormat]::Png)
            } finally { $graphics.Dispose(); $bitmap.Dispose() }
        }
    } finally { $source.Dispose() }
    & $makeappx pack /d $stage /p $outputPath
    if ($LASTEXITCODE -ne 0) { throw 'MakeAppx validation/packaging failed.' }
    if ($TestSign) {
        $certificate = New-SelfSignedCertificate -Type Custom -KeyUsage DigitalSignature -Subject $Publisher `
            -CertStoreLocation 'Cert:\CurrentUser\My' -FriendlyName 'edoproxy temporary test signing' `
            -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3', '2.5.29.19={text}')
        & $signtool sign /fd SHA256 /sha1 $certificate.Thumbprint /s My $outputPath
        if ($LASTEXITCODE -ne 0) { throw 'MSIX signing failed.' }
        Export-Certificate -Cert $certificate -FilePath $certificatePath | Out-Null
        # MSIX verification needs the package SIP and a trusted certificate.
        # Trust only this temporary leaf certificate, and remove it even on failure.
        $trustPath = "Cert:\LocalMachine\TrustedPeople\$($certificate.Thumbprint)"
        $alreadyTrusted = Test-Path $trustPath
        try {
            if (-not $alreadyTrusted) {
                Import-Certificate -FilePath $certificatePath -CertStoreLocation 'Cert:\LocalMachine\TrustedPeople' | Out-Null
            }
            & $signtool verify /pa /v $outputPath
            if ($LASTEXITCODE -ne 0) { throw 'MSIX signature verification failed.' }
        } finally {
            if (-not $alreadyTrusted -and (Test-Path $trustPath)) {
                Remove-Item $trustPath
            }
        }
    }
    & $makeappx unpack /p $outputPath /d (Join-Path $work 'verify')
    if ($LASTEXITCODE -ne 0) { throw 'MSIX unpack verification failed.' }
    Write-Host "Created $outputPath (Cargo $version, MSIX $PackageVersion)"
} finally {
    if ($certificate) { Remove-Item "Cert:\CurrentUser\My\$($certificate.Thumbprint)" -DeleteKey }
    if (Test-Path $work) { Remove-Item $work -Recurse -Force }
}
