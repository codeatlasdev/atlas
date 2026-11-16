# Atlas installer for Windows: https://atlas.codeatlas.com.br
#   irm https://atlas.codeatlas.com.br/install.ps1 | iex
#   $env:ATLAS_CHANNEL = 'stable'; irm https://atlas.codeatlas.com.br/install.ps1 | iex   # stable, beta or alpha
#
# The Windows twin of install.sh: downloads the newest Stable GitHub Release (no prerelease label) of
# codeatlasdev/atlas that has a Windows build (else, on the alpha channel, the default channel's newest build), verifies its sha256 and runs the binary's own `atlas install`, which puts
# `atlas.exe` in ~\.atlas\bin, adds that folder to your user PATH and installs the bundled agent
# skills. Then the installed atlas moves to the channel you follow: the default (alpha while Atlas
# is in alpha, so the newest build from main) or the one in ATLAS_CHANNEL, which is remembered.
# No admin rights. Prebuilt binaries: Windows 10 1809+ / 11 on x64 (ARM64 runs them emulated).
# Atlas Desktop: `atlas desktop` afterwards, or the Atlas-*-setup.exe on the release page.
#
# Runs in Windows PowerShell 5.1 (what a stock Windows has) and PowerShell 7; scripts/install-ps1.test.ts
# runs it in both (CI job "Installer"). Keep it 5.1-safe: ASCII only (`irm` may decode it as Windows-1252),
# no `$IsWindows`, no `??` or `?:`, and never `2>$null` on a native command (see Invoke-Native). It is
# piped to `iex`, never run as a file, so the execution policy does not apply to it.

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$Repo = 'codeatlasdev/atlas'
$DefaultChannel = 'alpha'
# Test hook (scripts/install-ps1.test.ts): ATLAS_GITHUB_ORIGIN points every GitHub request at a local
# server and allows plain http for it. Never set it yourself; unset, everything is https://github.com.
$Origin = if ($env:ATLAS_GITHUB_ORIGIN) { $env:ATLAS_GITHUB_ORIGIN.TrimEnd('/') } else { 'https://github.com' }
$Proto = if ($env:ATLAS_GITHUB_ORIGIN) { '=http,https' } else { '=https' }

function Say([string]$m) { Write-Host "  $m" }
function Die([string]$m) { Write-Host "  x $m" -ForegroundColor Red; throw $m }

# Run a native command and return @{ Code; Out } (Out = its stdout and stderr as text). Windows PowerShell
# 5.1 turns every stderr line of a native command into an error record as soon as stderr is redirected,
# and under $ErrorActionPreference = 'Stop' the first one is a terminating NativeCommandError: curl's own
# "(22) ... 404" for a release without a Windows build ended the whole installer (#519). Here the
# preference is 'Continue' (function-scoped) and the exit code decides, never the error stream.
# $global:LASTEXITCODE: a plain $LASTEXITCODE inside a function reads a local copy.
function Invoke-Native([string]$Exe, [string[]]$NativeArgs) {
  $ErrorActionPreference = 'Continue'
  $global:LASTEXITCODE = $null
  $out = & $Exe @NativeArgs 2>&1 | ForEach-Object { "$_" }
  [pscustomobject]@{ Code = $global:LASTEXITCODE; Out = ($out -join "`n") }
}
function Invoke-Curl([string[]]$CurlArgs) {
  Invoke-Native 'curl.exe' (@('--proto', $Proto, '--tlsv1.2') + $CurlArgs)
}

$channel = $env:ATLAS_CHANNEL
if ($channel -and $channel -notin @('stable', 'beta', 'alpha')) {
  Die "unknown ATLAS_CHANNEL '$channel' (expected stable, beta or alpha)"
}

# A 32-bit PowerShell on 64-bit Windows reports x86 here; the real architecture is in ARCHITEW6432.
$arch = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
if ($arch -notin @('AMD64', 'ARM64')) {
  Die "Atlas ships prebuilt Windows binaries for x64; this is $arch. Build from source: https://github.com/$Repo/blob/main/docs/installation.md#build-from-source"
}
$target = 'x86_64-pc-windows-msvc'
if (-not (Get-Command 'curl.exe' -ErrorAction SilentlyContinue)) { Die 'curl.exe is required (Windows 10 1803 or newer ships it)' }
# Windows' own tar (bsdtar): a GNU tar earlier on PATH (Git for Windows) reads `C:\...` as a remote host.
$tar = Join-Path $env:SystemRoot 'System32\tar.exe'
if (-not (Test-Path $tar)) {
  $found = Get-Command 'tar.exe' -ErrorAction SilentlyContinue
  if (-not $found) { Die 'tar.exe is required (Windows 10 1803 or newer ships it)' }
  $tar = $found.Source
}

# Newest Stable tag: the releases feed and the /releases/latest redirect (no API call, so no 60
# requests/hour limit). Pre-GA the redirect is an alpha, so it is only one more candidate.
function Get-Tags([string]$url) {
  $r = Invoke-Curl @('-fsSL', $url)
  if ($r.Code -ne 0) { return @() }
  [regex]::Matches($r.Out, 'href="[^"]*/releases/tag/([^"]*)"') | ForEach-Object {
    $_.Groups[1].Value -replace '%2[Bb]', '+'
  }
}
function Get-Stable([string[]]$tags) {
  $tags | Where-Object { $_ -match '^v?(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$' } |
    Sort-Object { [version]($_ -replace '^v', '') } -Descending | Select-Object -First 1
}

# The redirect's target is in its Location header; curl is used (not Invoke-WebRequest, which in 5.1
# throws on the 302 and hides the header) so both PowerShells see the same thing.
$latest = ''
$head = Invoke-Curl @('-sSI', "$Origin/$Repo/releases/latest")
if ($head.Code -eq 0 -and $head.Out -match '(?im)^location:\s*\S*/releases/tag/(\S+)') {
  $latest = $Matches[1] -replace '%2[Bb]', '+'
}

$seen = @(Get-Tags "$Origin/$Repo/releases.atom")
$tag = Get-Stable ($seen + $latest)
$pages = 0
while (-not $tag -and $pages -lt 5 -and $seen.Count -ge 10) {
  $page = @(Get-Tags "$Origin/$Repo/releases.atom?after=$($seen[-1])")
  if ($page.Count -eq 0) { break }
  $seen += $page
  $tag = Get-Stable ($seen + $latest)
  $pages++
}
$stable = $tag

# A Stable can predate a platform (1.4.0 has no Windows build; #91 was the same on Linux). Start from the
# first candidate whose release has the build: the newest Stable, then, on the alpha channel (the
# default), the /releases/latest redirect, the default channel's newest build. Stable and beta never
# get an alpha. Same rules as bootstrap_candidates/first_with_asset in install.sh. A 404 here is the
# expected answer "no", so it is a return value, never an error.
function Test-Asset([string]$t) {
  $v = $t -replace '^v', ''
  if (-not $v -or $v -notmatch '^[0-9A-Za-z._-]+$') { return $false }
  $r = Invoke-Curl @('-fsSIL', '-o', 'NUL', "$Origin/$Repo/releases/download/$t/atlas-$v-$target.tar.gz")
  return $r.Code -eq 0
}
$step = if ($channel) { $channel } else { $DefaultChannel }
$candidates = @($stable)
if ($step -eq 'alpha' -and $latest -and $latest -ne $stable) { $candidates += $latest }
$tag = $candidates | Where-Object { $_ } | Where-Object { Test-Asset $_ } | Select-Object -First 1
if (-not $tag) {
  if ($step -in @('stable', 'beta')) {
    Die "no Stable release has a build for $target yet (newest Stable: $(if ($stable) { $stable } else { 'none' })). Install the newest alpha instead:
    `$env:ATLAS_CHANNEL = 'alpha'; irm https://atlas.codeatlas.com.br/install.ps1 | iex"
  }
  Die "could not find a release with a build for ${target}: https://github.com/$Repo/releases"
}
if ($tag -ne $stable) { Say "no Stable build for $target yet (newest Stable: $(if ($stable) { $stable } else { 'none' })); starting from $tag" }
$version = $tag -replace '^v', ''

$asset = "atlas-$version-$target.tar.gz"
$base = "$Origin/$Repo/releases/download/$tag"
$tmp = Join-Path ([IO.Path]::GetTempPath()) ("atlas-install-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
  Say "Atlas $version ($target)"
  $r = Invoke-Curl @('-fsSL', '-o', "$tmp\$asset", "$base/$asset")
  if ($r.Code -ne 0) { Die "download failed: $asset`n    $($r.Out)" }
  $r = Invoke-Curl @('-fsSL', '-o', "$tmp\$asset.sha256", "$base/$asset.sha256")
  if ($r.Code -ne 0) { Die "download failed: $asset.sha256`n    $($r.Out)" }

  # Get-Content -Raw of an empty file is $null in 5.1, and $null.Trim() throws.
  $text = Get-Content "$tmp\$asset.sha256" -Raw
  $want = if ($text) { ($text.Trim() -split '\s+')[0].ToLowerInvariant() } else { '' }
  $got = (Get-FileHash -Algorithm SHA256 "$tmp\$asset").Hash.ToLowerInvariant()
  if (-not $want -or $want -ne $got) { Die "sha256 mismatch for $asset (expected $want, got $got)" }
  Say 'sha256 verified'

  $r = Invoke-Native $tar @('-xzf', "$tmp\$asset", '-C', $tmp, 'atlas.exe')
  if ($r.Code -ne 0) { Die "could not extract atlas.exe from ${asset}: $($r.Out)" }

  # atlas's own output goes straight to the console (no redirection, so it keeps its terminal); 'Continue'
  # only keeps a host that turns stderr into error records (the ISE) from stopping on its first line.
  $ErrorActionPreference = 'Continue'
  & "$tmp\atlas.exe" install
  $rc = $LASTEXITCODE
  $ErrorActionPreference = 'Stop'
  if ($rc -ne 0) { Die 'atlas install failed' }
} finally {
  Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}

# Same channel step as install.sh: without ATLAS_CHANNEL follow the default channel (not saved);
# with one, `--channel` saves it.
$bin = Join-Path $HOME '.atlas\bin'
$atlas = Join-Path $bin 'atlas.exe'
$ErrorActionPreference = 'Continue'
if (-not $channel) {
  Say "updating to the newest build of the $DefaultChannel channel"
  $env:ATLAS_UPDATE_CHANNEL = $DefaultChannel
  & $atlas update
  if ($LASTEXITCODE -ne 0) { Say 'could not update now; run later: atlas update' }
  Remove-Item Env:\ATLAS_UPDATE_CHANNEL -ErrorAction SilentlyContinue
} else {
  if ($channel -ne 'stable') { Say "switching to the $channel channel" }
  & $atlas update --channel $channel
  if ($LASTEXITCODE -ne 0) { Say "could not switch to $channel now; run later: atlas update --channel $channel" }
}
$ErrorActionPreference = 'Stop'

# `atlas install` added the folder to the user PATH, which a window already open does not see: add it to
# this one, so `atlas` works right now.
if (($env:Path -split ';') -notcontains $bin) { $env:Path = "$bin;$env:Path" }

Say ''
Say 'Next:'
Say '  atlas version               works in this window; other open terminals need to be reopened'
Say '  atlas desktop               install the desktop app'
Say '  atlas setup claude-code     wire an agent: omp | claude-code | codex (add --apply to write)'
Say '  docs: https://atlas.codeatlas.com.br/docs/getting-started/'
