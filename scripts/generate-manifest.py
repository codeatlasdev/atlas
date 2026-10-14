#!/usr/bin/env python3
"""Generate manifest.json from release artifacts.

Usage: generate-manifest.py <version> <artifacts-dir>
Optional environment: ATLAS_COMMIT (default: GITHUB_SHA, then `git rev-parse HEAD`),
ATLAS_PROMOTED_FROM (the prerelease tag a stable release was promoted from).

`prerelease` means "the version carries a SemVer prerelease label" (X.Y.Z-alpha.N), which is what clients and the
site badge as Alpha/Beta/RC. It is NOT GitHub's Release flag: release.yml publishes a build of the default channel
(alpha pre-GA) as a normal, "Latest" release (scripts/release-version.ts github-flags).
"""
import json
import re
import subprocess
import sys
import os
from datetime import datetime, timezone

VERSION_RE = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-(alpha|beta|rc)\.(0|[1-9]\d*))?$")
# rc is offered on the beta channel (docs/versioning.md).
CHANNEL_BY_LABEL = {None: "stable", "alpha": "alpha", "beta": "beta", "rc": "beta"}


def channel_of(version):
    m = VERSION_RE.match(version)
    if not m:
        raise ValueError(f"invalid version {version!r}: expected X.Y.Z or X.Y.Z-(alpha|beta|rc).N")
    return CHANNEL_BY_LABEL[m.group(4)]


def commit():
    sha = os.environ.get("ATLAS_COMMIT") or os.environ.get("GITHUB_SHA")
    if sha:
        return sha
    return subprocess.run(["git", "rev-parse", "HEAD"], capture_output=True, text=True, check=True,
                          cwd=os.path.dirname(os.path.abspath(__file__))).stdout.strip()


def get_sha256(filepath):
    sha_file = filepath + ".sha256"
    if os.path.exists(sha_file):
        with open(sha_file) as f:
            return f.read().strip().split()[0]
    return ""

def desktop_files(version, platform_key):
    """(manifest key, asset name) of the Atlas Desktop assets of a platform. `desktop` is what
    `atlas desktop` installs (`asset_name` in crates/atlas-cli/src/commands/desktop.rs); the rest are
    for people who download a package by hand."""
    if platform_key.endswith("-apple-darwin"):
        return (("desktop", f"Atlas-{version}-{platform_key}.app.tar.gz"),
                ("desktop_dmg", f"Atlas-{version}-{platform_key}.dmg"))
    if platform_key.endswith("-pc-windows-msvc"):
        # The per-user NSIS installer: `atlas desktop` runs it silently, people double-click it.
        return (("desktop", f"Atlas-{version}-{platform_key}-setup.exe"),)
    deb_arch, rpm_arch = {"x86_64": ("amd64", "x86_64"), "aarch64": ("arm64", "aarch64")}[platform_key.split("-")[0]]
    return (("desktop", f"Atlas-{version}-{platform_key}.AppImage"),
            ("desktop_deb", f"atlas_{version}_{deb_arch}.deb"),
            ("desktop_rpm", f"atlas-{version}-1.{rpm_arch}.rpm"),
            ("desktop_arch", f"codeatlas-atlas-bin-{version.replace('-', '', 1)}-1-{rpm_arch}.pkg.tar.zst"))

def desktop_assets(dir_path, version, platform_key):
    """The Atlas Desktop assets of a platform that exist in `dir_path`."""
    out = {}
    for key, name in desktop_files(version, platform_key):
        path = os.path.join(dir_path, name)
        if os.path.exists(path):
            out[key] = {
                "url": f"https://github.com/codeatlasdev/atlas/releases/download/v{version}/{name}",
                "sha256": get_sha256(path),
                "size": os.path.getsize(path),
            }
    return out

def main():
    version = sys.argv[1]
    artifacts_dir = sys.argv[2]

    platforms = [
        ("aarch64-apple-darwin", "atlas-aarch64-apple-darwin"),
        ("x86_64-apple-darwin", "atlas-x86_64-apple-darwin"),
        ("universal-apple-darwin", "atlas-universal-apple-darwin"),
        ("x86_64-unknown-linux-gnu", "atlas-x86_64-unknown-linux-gnu"),
        ("x86_64-pc-windows-msvc", "atlas-x86_64-pc-windows-msvc"),
    ]

    assets = {}
    for platform_key, artifact_dir in platforms:
        dir_path = os.path.join(artifacts_dir, artifact_dir)
        if not os.path.isdir(dir_path):
            continue

        tarball = f"atlas-{version}-{platform_key}.tar.gz"
        tarball_path = os.path.join(dir_path, tarball)

        if not os.path.exists(tarball_path):
            continue

        size = os.path.getsize(tarball_path)
        sha256 = get_sha256(tarball_path)

        assets[platform_key] = {
            "cli": {
                "url": f"https://github.com/codeatlasdev/atlas/releases/download/v{version}/{tarball}",
                "sha256": sha256,
                "size": size,
            },
            **desktop_assets(dir_path, version, platform_key),
            "daemon": {
                "url": f"https://github.com/codeatlasdev/atlas/releases/download/v{version}/{tarball}",
                "sha256": sha256,
                "size": size,
            },
        }

    channel = channel_of(version)
    manifest = {
        "version": version,
        "channel": channel,
        "prerelease": channel != "stable",
        "commit": commit(),
        "date": datetime.now(timezone.utc).isoformat(),
        "min_os": "14.0",
        "assets": assets,
        "changelog": f"https://github.com/codeatlasdev/atlas/releases/tag/v{version}",
        "required_update": False,
    }
    promoted_from = os.environ.get("ATLAS_PROMOTED_FROM")
    if promoted_from:
        manifest["promoted_from"] = promoted_from

    print(json.dumps(manifest, indent=2))

if __name__ == "__main__":
    main()
