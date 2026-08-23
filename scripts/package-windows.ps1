<#
.SYNOPSIS
    Build and package Zervo for Windows.

.DESCRIPTION
    Produces target/windows/Zervo-<version>-windows-<arch>/ containing the exe
    and everything it needs beside it, plus a .zip of that directory and,
    with -Installer, an NSIS setup executable.

    Nothing here is signed. SmartScreen will show "Windows protected your PC"
    on first run, and on a machine with Smart App Control enabled the binary
    will not run at all. See docs/PACKAGING.md.

.PARAMETER CargoProfile
    Cargo profile. Default release. Named CargoProfile because $Profile is a
    PowerShell automatic variable.

.PARAMETER Features
    Comma-separated cargo features.

.PARAMETER Target
    Rust target triple, for a cross or non-host build.

.PARAMETER Output
    Directory the artefacts land in. Default target/windows.

.PARAMETER NoBuild
    Package a binary that has already been built.

.PARAMETER Installer
    Also build an NSIS installer. Needs makensis on PATH (NSIS 3.10 is
    preinstalled on both the x64 and arm64 GitHub runner images).

.PARAMETER GStreamerRoot
    Root of an MSVC GStreamer installation, e.g.
    C:\gstreamer\1.0\msvc_x86_64. When given, its libraries and plugins are
    copied beside the exe so audio and video work on a machine that has never
    heard of GStreamer. Pair it with -Features media.

.EXAMPLE
    pwsh scripts/package-windows.ps1 -Features engine-downloads -Installer
#>
[CmdletBinding()]
param(
    [string] $CargoProfile = 'release',
    [string] $Features = '',
    [string] $Target   = '',
    [string] $Output   = '',
    [switch] $NoBuild,
    [switch] $Installer,
    [string] $GStreamerRoot = ''
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true

function Say  { param($m) Write-Host "==> $m" -ForegroundColor Cyan }
function Note { param($m) Write-Host "    $m" }
function Die  { param($m) Write-Error $m; exit 1 }

$RepoRoot = Split-Path -Parent (Split-Path -Parent $PSCommandPath)
Set-Location $RepoRoot

# ── Version, out of the [package] section specifically ────────────────────
# Not the first `version =` in the file: a [workspace.package] block would sit
# above [package] and win, and stamp the wrong number onto the installer.
$inPackage = $false
$Version = $null
foreach ($line in Get-Content Cargo.toml) {
    if ($line -match '^\[package\]') { $inPackage = $true; continue }
    if ($line -match '^\[')          { $inPackage = $false; continue }
    if ($inPackage -and $line -match '^version\s*=\s*"([^"]+)"') { $Version = $Matches[1]; break }
}
if (-not $Version) { Die 'no version in the [package] section of Cargo.toml' }

# NSIS's VIProductVersion takes exactly four numeric components, and the
# installer script builds it as "$Version.0". Anything that is not three plain
# numbers — a 0.5.0-rc.1, or the 0.3.3.1 this repository tagged once — makes
# makensis fail at the end of the build rather than the start of it.
if ($Installer -and $Version -notmatch '^\d+\.\d+\.\d+$') {
    Die "the NSIS installer needs a three-component numeric version; Cargo.toml says $Version"
}

# ── Where cargo put things ────────────────────────────────────────────────
$TargetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { 'target' }
$ProfileDir = switch ($CargoProfile) {
    'dev'     { 'debug' }
    'test'    { 'debug' }
    'release' { 'release' }
    'bench'   { 'release' }
    default   { $CargoProfile }
}
$BuildDir = if ($Target) { Join-Path $TargetDir $Target } else { $TargetDir }
$BuildDir = Join-Path $BuildDir $ProfileDir

$Arch = if ($Target) { ($Target -split '-')[0] } else { $env:PROCESSOR_ARCHITECTURE }
if (-not $Arch) { $Arch = 'x64' }
$Arch = switch ($Arch) {
    'AMD64'   { 'x64' }
    'x86_64'  { 'x64' }
    'ARM64'   { 'arm64' }
    'aarch64' { 'arm64' }
    default   { $Arch.ToLower() }
}
if (-not $Output) { $Output = Join-Path $TargetDir 'windows' }

$what = "profile: $CargoProfile"
if ($Features) { $what += ", features: $Features" }
Say "Zervo $Version for windows-$Arch ($what)"

# ── Build ─────────────────────────────────────────────────────────────────
if (-not $NoBuild) {
    # --locked, because a release must resolve the graph Cargo.lock describes
    # and nothing else.
    $cargoArgs = @('build', '--profile', $CargoProfile, '--locked')
    if ($Features) { $cargoArgs += @('--features', $Features) }
    if ($Target)   { $cargoArgs += @('--target', $Target) }
    Say "cargo $($cargoArgs -join ' ')"
    & cargo @cargoArgs
    if ($LASTEXITCODE -ne 0) { Die "cargo build failed with $LASTEXITCODE" }
}

$Exe = Join-Path $BuildDir 'zervo.exe'
if (-not (Test-Path $Exe)) { Die "no binary at $Exe" }

# ── Stage ─────────────────────────────────────────────────────────────────
$Name  = "Zervo-$Version-windows-$Arch"
$Stage = Join-Path $Output $Name
if (Test-Path $Stage) { Remove-Item -Recurse -Force $Stage }
New-Item -ItemType Directory -Force -Path $Stage | Out-Null

Say 'staging'
Copy-Item $Exe (Join-Path $Stage 'Zervo.exe')
Copy-Item LICENSE   (Join-Path $Stage 'LICENSE.txt')
Copy-Item README.md (Join-Path $Stage 'README.md')

# Anything the build already dropped beside the binary. Most of the engine
# links statically; whatever does not has to travel with the exe.
Get-ChildItem $BuildDir -Filter *.dll -ErrorAction SilentlyContinue |
    ForEach-Object { Copy-Item $_.FullName $Stage }

# ANGLE. Windows composites through EGL on Direct3D 11 rather than WGL (the
# `no-wgl` engine feature), and mozangle builds libEGL and libGLESv2 into its
# own OUT_DIR, not into the profile directory. The exe is linked against
# libEGL, so without these two it does not start at all.
#
# Sorted newest-first rather than taking whatever the filesystem hands back
# first: a target directory that has built more than one mozangle version
# contains more than one libEGL.dll, and the stale one is a crash at launch.
$angle = Get-ChildItem (Join-Path $BuildDir 'build') -Recurse -Filter libEGL.dll -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending
if (-not $angle) { Die "ANGLE DLLs not found under $BuildDir\build — did mozangle build?" }
Note "ANGLE from $($angle[0].DirectoryName)"
Get-ChildItem $angle[0].DirectoryName -Filter *.dll | ForEach-Object { Copy-Item $_.FullName $Stage -Force }

# The Visual C++ runtime. Rust's MSVC targets link the CRT dynamically, so a
# machine without the redistributable installed gets "vcruntime140.dll was not
# found" and nothing else. Shipping the two DLLs beside the exe is what Servo
# does, and it is the only option that does not make the user run an installer
# before the installer.
$redistRoot = $env:VCToolsRedistDir
if (-not $redistRoot) {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (Test-Path $vswhere) {
        $vs = & $vswhere -latest -products '*' -property installationPath
        if ($vs) {
            # The whole redist tree, not a version directory picked out of it.
            # Sorting those names descending chose `v145` over `14.44.35112` on
            # the VS2026 image — 'v' sorts after a digit — and there is no
            # x64 directory under it, so the search below found nothing and the
            # package went out without a C runtime.
            $candidate = Join-Path $vs 'VC\Redist\MSVC'
            if (Test-Path $candidate) { $redistRoot = $candidate }
        }
    }
}
if ($redistRoot) {
    # Recursed from the root and filtered by architecture, because the layout
    # below it has changed with every Visual Studio and will change again. The
    # 140 in these names is the ABI version, which has been stable since VS2015
    # and is not the toolset version.
    $crt = Get-ChildItem $redistRoot -Recurse -File -Include 'msvcp140.dll','vcruntime140.dll','vcruntime140_1.dll' -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -like "*\$Arch\*" } |
        Sort-Object LastWriteTime -Descending |
        Group-Object Name | ForEach-Object { $_.Group[0] }
    $copied = 0
    foreach ($dll in $crt) { Copy-Item $dll.FullName $Stage -Force; Note "CRT $($dll.Name)"; $copied++ }
    if ($copied -eq 0) {
        Write-Warning "no $Arch Visual C++ runtime DLLs under $redistRoot; the package will need the redistributable installed"
    }
} else {
    Write-Warning 'Visual C++ redistributable DLLs not found; the zip will need the redist installed on the target machine'
}

# ── GStreamer ─────────────────────────────────────────────────────────────
# Unlike Linux, where GStreamer is an ordinary system package the .deb can
# simply depend on, a Windows machine has no GStreamer unless something puts it
# there. Servo's answer is to copy the libraries beside the exe, and the
# plugins FLAT beside them rather than into lib/gstreamer-1.0 — the engine
# looks for them next to the binary, and a tidier layout means media silently
# stops working with no error to point at.
#
# Everything in bin/ and everything in lib/gstreamer-1.0/ is copied, rather
# than the curated fifty-odd names Servo maintains by hand. That list is
# smaller and rots: a plugin is loaded by name at runtime and never appears in
# any dependency scan, so a missing one is a feature that quietly does not
# work. Disk is the cheaper thing to spend here.
if ($GStreamerRoot) {
    if (-not (Test-Path $GStreamerRoot)) { Die "no GStreamer at $GStreamerRoot" }
    Say "bundling GStreamer from $GStreamerRoot"

    $gstBin = Join-Path $GStreamerRoot 'bin'
    $gstPlugins = Join-Path $GStreamerRoot 'lib\gstreamer-1.0'
    if (-not (Test-Path $gstBin)) { Die "no bin directory under $GStreamerRoot" }

    $n = 0
    Get-ChildItem $gstBin -Filter *.dll | ForEach-Object {
        Copy-Item $_.FullName $Stage -Force; $n++
    }
    if (Test-Path $gstPlugins) {
        Get-ChildItem $gstPlugins -Filter *.dll | ForEach-Object {
            Copy-Item $_.FullName $Stage -Force; $n++
        }
    } else {
        Write-Warning "no lib\gstreamer-1.0 under $GStreamerRoot; media will not play"
    }
    Note "GStreamer: $n DLLs"
}

Get-ChildItem $Stage | Format-Table Name, Length

# ── Zip ───────────────────────────────────────────────────────────────────
# bsdtar rather than Compress-Archive. Compress-Archive is backed by
# System.IO.Compression, reads the whole payload through memory, is slow on a
# few hundred megabytes of GStreamer, and fails past 2 GB with an exception
# rather than a message. bsdtar ships with every supported Windows and infers
# zip from the extension. Compress-Archive stays as the fallback for a machine
# that somehow has no tar.
Say 'zip'
$Zip = Join-Path $Output "$Name.zip"
if (Test-Path $Zip) { Remove-Item -Force $Zip }
# Only bsdtar. A GNU tar from Git-for-Windows or MSYS is also called `tar`,
# also accepts -a, and cannot write a zip at all — so the check is for what it
# is, not for whether something by that name exists.
$tar = Get-Command tar -ErrorAction SilentlyContinue
$isBsdTar = $false
if ($tar) {
    try { $isBsdTar = (& $tar.Source --version 2>&1 | Out-String) -match 'bsdtar' } catch { $isBsdTar = $false }
}
if ($isBsdTar) {
    & $tar.Source -a -c -f $Zip -C $Output $Name
    if ($LASTEXITCODE -ne 0) { Die "tar failed with $LASTEXITCODE" }
} else {
    Compress-Archive -Path $Stage -DestinationPath $Zip
}
(Get-FileHash $Zip -Algorithm SHA256).Hash.ToLower() + '  ' + (Split-Path -Leaf $Zip) | Write-Host
Note $Zip

# ── Installer ─────────────────────────────────────────────────────────────
if ($Installer) {
    Say 'installer'
    # NSIS ships on both Windows runner images and is not on PATH there, so
    # "not found" meant the release quietly went out without the installer it
    # had been asked for.
    # Written out rather than chained: under Set-StrictMode a property read on
    # a null Get-Command result, or a Join-Path with a null root, is a
    # terminating error rather than an empty answer.
    $makensisCmd = Get-Command makensis -ErrorAction SilentlyContinue
    $makensisPath = if ($makensisCmd) { $makensisCmd.Source } else { $null }
    if (-not $makensisPath) {
        foreach ($root in @(${env:ProgramFiles(x86)}, $env:ProgramFiles)) {
            if (-not $root) { continue }
            $guess = Join-Path $root 'NSIS\makensis.exe'
            if (Test-Path $guess) { $makensisPath = $guess; break }
        }
    }
    if (-not $makensisPath) {
        # Asked for explicitly, so its absence is an error rather than a note.
        Die 'makensis not found: install NSIS, or drop -Installer'
    } else {
        Note "makensis at $makensisPath"
        $SetupName = "Zervo-$Version-windows-$Arch-setup.exe"
        # /NOCD first, and it must come before the script name: makensis
        # otherwise changes directory to the .nsi file's own, and both the
        # staging directory and the output path handed to it are relative to
        # the repository root. Without this the documented local invocation —
        # which does not pass -Output — fails inside NSIS with "no files
        # found", pointing at scripts\windows\target\windows.
        & $makensisPath `
            '/NOCD' `
            "/DVERSION=$Version" `
            "/DARCH=$Arch" `
            "/DSTAGE=$Stage" `
            "/DOUTFILE=$(Join-Path $Output $SetupName)" `
            "$RepoRoot\scripts\windows\zervo.nsi"
        if ($LASTEXITCODE -ne 0) { Die "makensis failed with $LASTEXITCODE" }
        $Setup = Join-Path $Output $SetupName
        (Get-FileHash $Setup -Algorithm SHA256).Hash.ToLower() + '  ' + $SetupName | Write-Host
        Note $Setup
    }
}

Say "in $Output"
Get-ChildItem $Output | Format-Table Name, Length
