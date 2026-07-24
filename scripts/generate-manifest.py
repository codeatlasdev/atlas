#!/usr/bin/env python3
"""Generate manifest.json from release artifacts."""
import json
import sys
import os
from datetime import datetime, timezone

def get_sha256(filepath):
    sha_file = filepath + ".sha256"
    if os.path.exists(sha_file):
        with open(sha_file) as f:
            return f.read().strip().split()[0]
    return ""

def main():
    version = sys.argv[1]
    artifacts_dir = sys.argv[2]

    platforms = [
        ("aarch64-apple-darwin", "atlas-aarch64-apple-darwin"),
        ("x86_64-apple-darwin", "atlas-x86_64-apple-darwin"),
        ("universal-apple-darwin", "atlas-universal-apple-darwin"),
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
            "daemon": {
                "url": f"https://github.com/codeatlasdev/atlas/releases/download/v{version}/{tarball}",
                "sha256": sha256,
                "size": size,
            },
        }

    manifest = {
        "version": version,
        "channel": "beta" if "beta" in version else "stable",
        "date": datetime.now(timezone.utc).isoformat(),
        "min_os": "14.0",
        "assets": assets,
        "changelog": f"https://github.com/codeatlasdev/atlas/releases/tag/v{version}",
        "required_update": False,
    }

    print(json.dumps(manifest, indent=2))

if __name__ == "__main__":
    main()
