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

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$Repo = 'codeatlasdev/atlas'
$DefaultChannel = 'alpha'

function Say([string]$m) { Write-Host "  $m" }
function Die([string]$m) { Write-Host "  x $m" -ForegroundColor Red; throw $m }

$channel = $env:ATLAS_CHANNEL
if ($channel -and $channel -notin @('stable', 'beta', 'alpha')) {
  Die "unknown ATLAS_CHANNEL '$channel' (expected stable, beta or alpha)"
}

$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -notin @('AMD64', 'ARM64')) {
  Die "Atlas ships prebuilt Windows binaries for x64; this is $arch. Build from source: https://github.com/$Repo/blob/main/docs/installation.md#build-from-source"
}
$target = 'x86_64-pc-windows-msvc'
foreach ($tool in @('tar.exe', 'curl.exe')) {
  if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { Die "$tool is required (Windows 10 1809 or newer ships it)" }
}

# Newest Stable tag: the releases feed and the /releases/latest redirect (no API call, so no 60
# requests/hour limit). Pre-GA the redirect is an alpha, so it is only one more candidate.
function Get-Tags([string]$url) {
  try {
    $xml = (Invoke-WebRequest -UseBasicParsing -Uri $url).Content
  } catch { return @() }
  [regex]::Matches($xml, 'href="[^"]*/releases/tag/([^"]*)"') | ForEach-Object {
    $_.Groups[1].Value -replace '%2[Bb]', '+'
  }
}
function Get-Stable([string[]]$tags) {
  $tags | Where-Object { $_ -match '^v?(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$' } |
    Sort-Object { [version]($_ -replace '^v', '') } -Descending | Select-Object -First 1
}

$latest = ''
try {
  $r = Invoke-WebRequest -UseBasicParsing -Uri "https://github.com/$Repo/releases/latest" -MaximumRedirection 0 -ErrorAction SilentlyContinue
  $loc = $r.Headers.Location
} catch { $loc = $_.Exception.Response.Headers.Location }
if ($loc -and "$loc" -match '/releases/tag/(.+)$') { $latest = $Matches[1] }

$seen = @(Get-Tags "https://github.com/$Repo/releases.atom")
$tag = Get-Stable ($seen + $latest)
$pages = 0
while (-not $tag -and $pages -lt 5 -and $seen.Count -ge 10) {
  $page = @(Get-Tags "https://github.com/$Repo/releases.atom?after=$($seen[-1])")
  if ($page.Count -eq 0) { break }
  $seen += $page
  $tag = Get-Stable ($seen + $latest)
  $pages++
}
$stable = $tag

# A Stable can predate a platform (1.4.0 has no Windows build; #91 was the same on Linux). Start from the
# first candidate whose release has the build: the newest Stable, then, on the alpha channel (the
# default), the /releases/latest redirect, the default channel's newest build. Stable and beta never
# get an alpha. Same rules as bootstrap_candidates/first_with_asset in install.sh.
function Test-Asset([string]$t) {
  $v = $t -replace '^v', ''
  if (-not $v -or $v -notmatch '^[0-9A-Za-z._-]+$') { return $false }
  & curl.exe -fsSIL --proto '=https' --tlsv1.2 -o NUL "https://github.com/$Repo/releases/download/$t/atlas-$v-$target.tar.gz" 2>$null
  return $LASTEXITCODE -eq 0
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
$base = "https://github.com/$Repo/releases/download/$tag"
$tmp = Join-Path ([IO.Path]::GetTempPath()) ("atlas-install-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
  Say "Atlas $version ($target)"
  & curl.exe -fsSL --proto '=https' --tlsv1.2 -o "$tmp\$asset" "$base/$asset"
  if ($LASTEXITCODE -ne 0) { Die "download failed: $asset" }
  & curl.exe -fsSL --proto '=https' --tlsv1.2 -o "$tmp\$asset.sha256" "$base/$asset.sha256"
  if ($LASTEXITCODE -ne 0) { Die "download failed: $asset.sha256" }

  $want = ((Get-Content "$tmp\$asset.sha256" -Raw).Trim() -split '\s+')[0].ToLowerInvariant()
  $got = (Get-FileHash -Algorithm SHA256 "$tmp\$asset").Hash.ToLowerInvariant()
  if (-not $want -or $want -ne $got) { Die "sha256 mismatch for $asset (expected $want, got $got)" }
  Say 'sha256 verified'

  & tar.exe -xzf "$tmp\$asset" -C "$tmp" atlas.exe
  if ($LASTEXITCODE -ne 0) { Die "could not extract atlas.exe from $asset" }

  & "$tmp\atlas.exe" install
  if ($LASTEXITCODE -ne 0) { Die 'atlas install failed' }
} finally {
  Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}

# Same channel step as install.sh: without ATLAS_CHANNEL follow the default channel (not saved);
# with one, `--channel` saves it.
$atlas = Join-Path $HOME '.atlas\bin\atlas.exe'
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

Say ''
Say 'Next:'
Say '  open a new terminal, then:  atlas version'
Say '  atlas desktop               install the desktop app'
Say '  atlas setup claude-code     wire an agent: omp | claude-code | codex (add --apply to write)'
Say '  docs: https://atlas.codeatlas.com.br/docs/getting-started/'
