#!/usr/bin/env python3
"""Run with: python3 -m unittest scripts/test_generate_manifest.py"""
import hashlib
import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPT = os.path.join(HERE, "generate-manifest.py")


def load():
    spec = importlib.util.spec_from_file_location("generate_manifest", SCRIPT)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def write_asset(dir_path, name, data):
    path = os.path.join(dir_path, name)
    with open(path, "wb") as f:
        f.write(data)
    digest = hashlib.sha256(data).hexdigest()
    with open(path + ".sha256", "w") as f:
        f.write(f"{digest}  {name}\n")
    return digest


def make_artifacts(version):
    root = tempfile.mkdtemp()
    plat = os.path.join(root, "atlas-aarch64-apple-darwin")
    os.makedirs(plat)
    cli = write_asset(plat, f"atlas-{version}-aarch64-apple-darwin.tar.gz", b"cli")
    app = write_asset(plat, f"Atlas-{version}-aarch64-apple-darwin.app.tar.gz", b"app!")
    dmg = write_asset(plat, f"Atlas-{version}-aarch64-apple-darwin.dmg", b"dmg??")
    return root, cli, app, dmg


def run_cli(version, root, env=None):
    full_env = {k: v for k, v in os.environ.items() if not k.startswith("ATLAS_")}
    full_env.update(env or {})
    out = subprocess.run(
        [sys.executable, SCRIPT, version, root],
        capture_output=True, text=True, env=full_env, check=True,
    ).stdout
    return json.loads(out)


class Manifest(unittest.TestCase):
    def test_linux_lists_the_appimage_as_the_desktop_and_every_package(self):
        # The names release.yml uploads; `desktop` must be what `atlas desktop` downloads on Linux.
        version = "1.6.0-beta.2"
        root = tempfile.mkdtemp()
        plat = os.path.join(root, "atlas-x86_64-unknown-linux-gnu")
        os.makedirs(plat)
        cli = write_asset(plat, f"atlas-{version}-x86_64-unknown-linux-gnu.tar.gz", b"cli")
        img = write_asset(plat, f"Atlas-{version}-x86_64-unknown-linux-gnu.AppImage", b"img")
        deb = write_asset(plat, f"atlas_{version}_amd64.deb", b"deb")
        rpm = write_asset(plat, f"atlas-{version}-1.x86_64.rpm", b"rpm")
        pkg = write_asset(plat, "codeatlas-atlas-bin-1.6.0beta.2-1-x86_64.pkg.tar.zst", b"zst")
        a = run_cli(version, root)["assets"]["x86_64-unknown-linux-gnu"]
        self.assertEqual(a["cli"]["sha256"], cli)
        self.assertEqual(a["desktop"]["sha256"], img)
        self.assertTrue(a["desktop"]["url"].endswith(f"/v{version}/Atlas-{version}-x86_64-unknown-linux-gnu.AppImage"))
        self.assertEqual(a["desktop_deb"]["sha256"], deb)
        self.assertEqual(a["desktop_rpm"]["sha256"], rpm)
        self.assertEqual(a["desktop_arch"]["sha256"], pkg)
        self.assertNotIn("desktop_dmg", a)

    def test_windows_lists_the_cli_tarball_and_the_installer_as_the_desktop(self):
        # The names release.yml uploads; `desktop` must be what `atlas desktop` downloads on Windows.
        version = "1.6.0-alpha.1"
        root = tempfile.mkdtemp()
        plat = os.path.join(root, "atlas-x86_64-pc-windows-msvc")
        os.makedirs(plat)
        cli = write_asset(plat, f"atlas-{version}-x86_64-pc-windows-msvc.tar.gz", b"cli")
        exe = write_asset(plat, f"Atlas-{version}-x86_64-pc-windows-msvc-setup.exe", b"MZexe")
        a = run_cli(version, root)["assets"]["x86_64-pc-windows-msvc"]
        self.assertEqual(a["cli"]["sha256"], cli)
        self.assertEqual(a["desktop"]["sha256"], exe)
        self.assertTrue(a["desktop"]["url"].endswith(f"/v{version}/Atlas-{version}-x86_64-pc-windows-msvc-setup.exe"))
        self.assertEqual(sorted(a), ["cli", "daemon", "desktop"])

    def test_stable_keeps_the_backwards_compatible_keys(self):
        root, cli, app, dmg = make_artifacts("1.4.0")
        m = run_cli("1.4.0", root, {"GITHUB_SHA": "abc123"})
        self.assertEqual(m["version"], "1.4.0")
        self.assertIn("date", m)
        self.assertEqual(m["channel"], "stable")
        self.assertFalse(m["prerelease"])
        a = m["assets"]["aarch64-apple-darwin"]
        self.assertEqual(a["cli"]["sha256"], cli)
        self.assertEqual(a["cli"]["size"], 3)
        self.assertEqual(a["desktop"]["sha256"], app)
        self.assertEqual(a["desktop_dmg"]["sha256"], dmg)
        self.assertEqual(a["daemon"]["url"], a["cli"]["url"])
        self.assertTrue(a["cli"]["url"].endswith("/download/v1.4.0/atlas-1.4.0-aarch64-apple-darwin.tar.gz"))
        self.assertNotIn("promoted_from", m)

    def test_prerelease_channel_comes_from_the_label(self):
        for version, channel in (
            ("1.4.0-alpha.1", "alpha"),
            ("1.4.0-alpha.10", "alpha"),
            ("1.4.0-beta.2", "beta"),
            ("1.4.0-rc.1", "beta"),
        ):
            with self.subTest(version=version):
                root, *_ = make_artifacts(version)
                m = run_cli(version, root)
                self.assertEqual(m["channel"], channel)
                self.assertTrue(m["prerelease"])
                self.assertEqual(m["version"], version)
                self.assertIn(f"/download/v{version}/", m["assets"]["aarch64-apple-darwin"]["cli"]["url"])
                self.assertEqual(m["changelog"], f"https://github.com/codeatlasdev/atlas/releases/tag/v{version}")

    # Incident guard: GitHub now publishes a default-channel alpha as a normal "Latest" release (release.yml),
    # so GitHub's prerelease flag no longer says "has a SemVer label". The manifest's `prerelease` keeps that
    # meaning (clients and the site read it to badge Alpha/Beta/RC), and it must NOT follow the GitHub flag.
    def test_prerelease_is_the_semver_label_not_github_s_release_flag(self):
        root, *_ = make_artifacts("1.5.0-alpha.4")
        m = run_cli("1.5.0-alpha.4", root, {"GITHUB_SHA": "abc"})
        self.assertTrue(m["prerelease"])
        self.assertEqual(m["channel"], "alpha")

    def test_commit_prefers_env_then_git(self):
        root, *_ = make_artifacts("1.4.0")
        self.assertEqual(run_cli("1.4.0", root, {"ATLAS_COMMIT": "deadbeef"})["commit"], "deadbeef")
        head = subprocess.run(["git", "rev-parse", "HEAD"], capture_output=True, text=True, cwd=HERE).stdout.strip()
        self.assertEqual(run_cli("1.4.0", root)["commit"], head)

    def test_promoted_from_is_recorded_when_given(self):
        root, *_ = make_artifacts("1.4.0")
        m = run_cli("1.4.0", root, {"ATLAS_PROMOTED_FROM": "v1.4.0-beta.2"})
        self.assertEqual(m["promoted_from"], "v1.4.0-beta.2")

    def test_empty_promoted_from_is_omitted(self):
        root, *_ = make_artifacts("1.4.0")
        self.assertNotIn("promoted_from", run_cli("1.4.0", root, {"ATLAS_PROMOTED_FROM": ""}))

    def test_channel_function_rejects_unknown_labels(self):
        mod = load()
        self.assertEqual(mod.channel_of("1.4.0"), "stable")
        self.assertEqual(mod.channel_of("1.4.0-alpha.1"), "alpha")
        with self.assertRaises(ValueError):
            mod.channel_of("1.4.0-nightly.1")
        with self.assertRaises(ValueError):
            mod.channel_of("1.4.0-alpha10")
        with self.assertRaises(ValueError):
            mod.channel_of("v1.4.0")

    def test_missing_artifacts_dir_gives_empty_assets(self):
        m = run_cli("1.4.0", tempfile.mkdtemp())
        self.assertEqual(m["assets"], {})


if __name__ == "__main__":
    unittest.main()
