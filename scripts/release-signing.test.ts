// Release signing (#279): the shell side of the trust chain and the release wiring. The Rust verifier has its own tests
// (crates/atlas-core/src/release_verify.rs); here the real `ssh-keygen` judges what the scripts produce and accept.
import { shArgv } from "./sh";
import { allowedSigners, type ReleaseKeys } from "./release-keys.ts";
import { describe, expect, test } from "bun:test";
import { chmodSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

const root = join(import.meta.dir, "..");
const read = (f: string) => readFileSync(join(root, f), "utf8");

function tmp(prefix: string) {
	return mkdtempSync(join(tmpdir(), `${prefix}-`));
}

function sh(argv: string[], opts: { cwd?: string; env?: Record<string, string | undefined>; stdin?: string } = {}) {
	const p = Bun.spawnSync(argv, { cwd: opts.cwd, env: { ...process.env, SSH_AUTH_SOCK: undefined, ...opts.env }, stdin: opts.stdin ? Buffer.from(opts.stdin) : undefined });
	return { code: p.exitCode, out: p.stdout.toString(), err: p.stderr.toString() };
}

/** A throwaway ed25519 key made by ssh-keygen. */
function keygen(dir: string, name: string) {
	const path = join(dir, name);
	const r = sh(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-C", name, "-f", path]);
	expect(r.code, r.err).toBe(0);
	return { path, pub: readFileSync(`${path}.pub`, "utf8").trim() };
}

function signFile(key: string, file: string, ns = "atlas-release") {
	rmSync(`${file}.sig`, { force: true }); // ssh-keygen would ask before overwriting (and exit 0 when it cannot ask)
	const r = sh(["ssh-keygen", "-Y", "sign", "-n", ns, "-f", key, file]);
	expect(r.code, r.err).toBe(0);
	return `${file}.sig`;
}

const SUMS = `${"a".repeat(64)}  atlas-1.0.0.tar.gz\n`;

describe("allowed_signers (what install.sh and release-sign.sh verify with)", () => {
	function check(policy: ReleaseKeys, key: string, opts: { ns?: string; principal?: string } = {}) {
		const dir = tmp("allowed");
		const sums = join(dir, "SHA256SUMS");
		writeFileSync(sums, SUMS);
		const sig = signFile(key, sums, opts.ns);
		writeFileSync(join(dir, "allowed_signers"), allowedSigners(policy));
		return sh(["ssh-keygen", "-Y", "verify", "-f", join(dir, "allowed_signers"), "-I", opts.principal ?? "alice", "-n", "atlas-release", "-s", sig], { stdin: SUMS });
	}

	test("a signature by a listed key verifies; another key, another namespace and a tampered file do not", () => {
		const dir = tmp("keys");
		const [alice, mallory] = [keygen(dir, "alice"), keygen(dir, "mallory")];
		const policy: ReleaseKeys = { signed_since: null, keys: [{ name: "alice", pubkey: alice.pub }] };
		expect(check(policy, alice.path).code).toBe(0);
		expect(check(policy, mallory.path).code).not.toBe(0);
		expect(check(policy, alice.path, { ns: "git" }).code).not.toBe(0);
		const sums = join(dir, "SHA256SUMS");
		writeFileSync(sums, SUMS);
		const sig = signFile(alice.path, sums);
		writeFileSync(join(dir, "allowed_signers"), allowedSigners(policy));
		const tampered = sh(["ssh-keygen", "-Y", "verify", "-f", join(dir, "allowed_signers"), "-I", "alice", "-n", "atlas-release", "-s", sig], { stdin: `${SUMS}x` });
		expect(tampered.code).not.toBe(0);
	});

	test("valid-after and valid-before are honoured (an expired key is refused by ssh-keygen too)", () => {
		const dir = tmp("keys");
		const alice = keygen(dir, "alice");
		const key = (extra: object) => ({ signed_since: null, keys: [{ name: "alice", pubkey: alice.pub, ...extra }] }) as ReleaseKeys;
		expect(check(key({ valid_before: "2999-01-01" }), alice.path).code).toBe(0);
		expect(check(key({ valid_before: "2001-01-01" }), alice.path).code).not.toBe(0);
		expect(check(key({ valid_after: "2999-01-01" }), alice.path).code).not.toBe(0);
	});

	test("a key that is not ssh-ed25519, or has an unsafe name, cannot be listed", () => {
		const dir = tmp("keys");
		const r = sh(["ssh-keygen", "-q", "-t", "ecdsa", "-N", "", "-f", join(dir, "e")]);
		expect(r.code, r.err).toBe(0);
		const ecdsa = readFileSync(join(dir, "e.pub"), "utf8").trim();
		expect(() => allowedSigners({ signed_since: null, keys: [{ name: "e", pubkey: ecdsa }] })).toThrow(/ssh-ed25519/);
		expect(() => allowedSigners({ signed_since: null, keys: [{ name: "a b", pubkey: "ssh-ed25519 AAAA" }] })).toThrow(/name/);
	});
});

/** A scratch checkout (`scripts/`, the keys file, channels.json) where `gh` is a stub that logs and serves fixtures. */
function scratch(keys: ReleaseKeys) {
	const dir = tmp("release-sign");
	for (const f of ["release-sign.sh", "release-keys.ts", "release-version.ts", "release.sh", "sh.ts"]) {
		mkdirSync(join(dir, "scripts"), { recursive: true });
		cpSync(join(root, "scripts", f), join(dir, "scripts", f));
	}
	for (const f of ["channels.json"]) {
		mkdirSync(join(dir, "crates/atlas-core/data"), { recursive: true });
		cpSync(join(root, "crates/atlas-core/data", f), join(dir, "crates/atlas-core/data", f));
	}
	writeFileSync(join(dir, "crates/atlas-core/data/release-keys.json"), JSON.stringify(keys));
	const fix = join(dir, "fixtures");
	mkdirSync(fix);
	const bin = join(dir, "bin");
	mkdirSync(bin);
	// `gh release view|download|upload|edit` over a directory of fixtures; every call is logged.
	writeFileSync(
		join(bin, "gh"),
		`#!/usr/bin/env bash
echo "gh $*" >> "${fix}/calls.log"
[ "$1 $2" = "release view" ] || [ "$1 $2" = "release download" ] || [ "$1 $2" = "release upload" ] || [ "$1 $2" = "release edit" ] || exit 1
case "$2" in
  view) case "$*" in
      *isDraft*) cat "${fix}/draft"; exit 0;;
      *assets*) cat "${fix}/assets"; exit 0;;
    esac; exit 0;;
  download) while [ $# -gt 0 ]; do case "$1" in -p) n="$2";; -D) d="$2";; esac; shift; done
    cp "${fix}/$n" "$d/$n";;
  upload) for a in "$@"; do f="$a"; done; cp "$f" "${fix}/$(basename "$f")"; basename "$f" >> "${fix}/assets";;
esac
`,
	);
	chmodSync(join(bin, "gh"), 0o755);
	writeFileSync(join(bin, "cargo"), "#!/bin/sh\nexit 0\n");
	chmodSync(join(bin, "cargo"), 0o755);
	writeFileSync(join(fix, "draft"), "true\n");
	writeFileSync(join(fix, "assets"), "SHA256SUMS\nmanifest.json\n");
	writeFileSync(join(fix, "SHA256SUMS"), SUMS);
	const calls = () => (existsSync(join(fix, "calls.log")) ? readFileSync(join(fix, "calls.log"), "utf8").split("\n").filter(Boolean) : []);
	const run = (argv: string[], env: Record<string, string> = {}) =>
		sh(shArgv(join(dir, "scripts/release-sign.sh"), ...argv), { cwd: dir, env: { PATH: `${bin}:${dirname(process.execPath)}:${process.env.PATH}`, ...env } });
	return { dir, fix, calls, run };
}

describe("release-sign.sh", () => {
	const keys = () => {
		const kd = tmp("keys");
		return { maintainer: keygen(kd, "maintainer"), attacker: keygen(kd, "attacker") };
	};
	const policy = (pub: string): ReleaseKeys => ({ signed_since: "v1.5.0-alpha.27", keys: [{ name: "maintainer", pubkey: pub }] });
	const published = (calls: string[]) => calls.some((c) => c.startsWith("gh release edit") && c.includes("--draft=false"));
	const uploaded = (calls: string[]) => calls.some((c) => c.startsWith("gh release upload"));

	test("signs SHA256SUMS, uploads the signature the real ssh-keygen accepts, then publishes the draft", () => {
		const k = keys();
		const s = scratch(policy(k.maintainer.pub));
		const r = s.run(["v1.5.0-alpha.27", "--key", k.maintainer.path, "--yes"]);
		expect(r.code, r.err + r.out).toBe(0);
		const calls = s.calls();
		const up = calls.findIndex((c) => c.startsWith("gh release upload"));
		const edit = calls.findIndex((c) => c.startsWith("gh release edit"));
		expect(up, calls.join("\n")).toBeGreaterThan(-1);
		expect(edit).toBeGreaterThan(up); // the signature is on the release before anyone can see it
		expect(calls[edit]).toContain("--draft=false");
		expect(calls[edit]).toContain("--latest=true"); // alphas are "Latest" while alpha is the default channel
		// Judged by the oracle, against the keys file's own allowed_signers.
		writeFileSync(join(s.dir, "allowed"), allowedSigners(policy(k.maintainer.pub)));
		const v = sh(["ssh-keygen", "-Y", "verify", "-f", join(s.dir, "allowed"), "-I", "maintainer", "-n", "atlas-release", "-s", join(s.fix, "SHA256SUMS.sig")], { stdin: SUMS });
		expect(v.code, v.err).toBe(0);
	});

	test("without a signing key nothing is uploaded and the draft stays a draft", () => {
		const k = keys();
		const s = scratch(policy(k.maintainer.pub));
		const r = s.run(["v1.5.0-alpha.27", "--yes"], { ATLAS_RELEASE_KEY: "" });
		expect(r.code).not.toBe(0);
		expect(r.err).toContain("no signing key");
		expect(uploaded(s.calls()) || published(s.calls())).toBe(false);
	});

	test("a signature by a key that is not in release-keys.json is never uploaded or published", () => {
		const k = keys();
		const s = scratch(policy(k.maintainer.pub));
		const r = s.run(["v1.5.0-alpha.27", "--key", k.attacker.path, "--yes"]);
		expect(r.code).not.toBe(0);
		expect(r.err).toContain("does not verify");
		expect(uploaded(s.calls()) || published(s.calls())).toBe(false);
	});

	test("a draft that already has a signature by a wrong key is refused; a good one is only published", () => {
		const k = keys();
		const s = scratch(policy(k.maintainer.pub));
		writeFileSync(join(s.fix, "assets"), "SHA256SUMS\nSHA256SUMS.sig\n");
		signFile(k.attacker.path, join(s.fix, "SHA256SUMS")); // writes fixtures/SHA256SUMS.sig, which the stub serves
		const bad = s.run(["v1.5.0-alpha.27", "--yes"]);
		expect(bad.code).not.toBe(0);
		expect(published(s.calls())).toBe(false);
		signFile(k.maintainer.path, join(s.fix, "SHA256SUMS"));
		const good = s.run(["v1.5.0-alpha.27", "--yes"]);
		expect(good.code, good.err).toBe(0);
		expect(uploaded(s.calls())).toBe(false);
		expect(published(s.calls())).toBe(true);
	});

	test("with no key configured yet the draft is published as before, with a warning", () => {
		const s = scratch({ signed_since: null, keys: [] });
		const r = s.run(["v1.5.0-alpha.27", "--yes"]);
		expect(r.code, r.err).toBe(0);
		expect(r.err).toContain("unsigned");
		expect(published(s.calls())).toBe(true);
	});

	test("a release that is already public is left alone", () => {
		const k = keys();
		const s = scratch(policy(k.maintainer.pub));
		writeFileSync(join(s.fix, "draft"), "false\n");
		writeFileSync(join(s.fix, "assets"), "SHA256SUMS\nSHA256SUMS.sig\n");
		signFile(k.maintainer.path, join(s.fix, "SHA256SUMS"));
		const r = s.run(["v1.5.0-alpha.27", "--yes"]);
		expect(r.code, r.err).toBe(0);
		expect(published(s.calls())).toBe(false);
	});
});

describe("release.sh", () => {
	// A scratch repo shaped like a soaked release branch: release/1.5 at v1.5.0-beta.1, origin a local bare repo.
	function repo() {
		const keys = keygen(tmp("keys"), "maintainer");
		const s = scratch({ signed_since: "v1.5.0-alpha.27", keys: [{ name: "maintainer", pubkey: keys.pub }] });
		const env = { GIT_CONFIG_GLOBAL: "/dev/null", GIT_CONFIG_NOSYSTEM: "1", XDG_CONFIG_HOME: undefined, PATH: `${s.dir}/bin:${dirname(process.execPath)}:${process.env.PATH}` };
		const git = (...a: string[]) => {
			const r = sh(["git", "-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false", ...a], { cwd: s.dir, env });
			expect(r.code, `git ${a.join(" ")}: ${r.err}`).toBe(0);
			return r.out;
		};
		const origin = join(tmp("origin"), "o.git");
		sh(["git", "init", "-q", "--bare", origin], { env });
		git("init", "-q", "-b", "main");
		mkdirSync(join(s.dir, "crates/atlas-cli"), { recursive: true });
		writeFileSync(join(s.dir, "crates/atlas-cli/Cargo.toml"), '[package]\nname = "atlas-cli"\nversion = "1.5.0-beta.1"\n');
		writeFileSync(join(s.dir, "CHANGELOG.md"), "# Changelog\n\n## [1.5.0] - 2026-11-16\n\n- Something.\n");
		writeFileSync(join(s.dir, ".gitignore"), "bin/\nfixtures/\n");
		git("add", "-A");
		git("commit", "-q", "-m", "init");
		git("remote", "add", "origin", origin);
		git("switch", "-q", "-c", "release/1.5");
		git("tag", "-a", "v1.5.0-beta.1", "-m", "beta");
		git("push", "-q", "origin", "main", "release/1.5", "--tags");
		return { ...s, env };
	}
	const releaseSh = (r: { dir: string; env: Record<string, string | undefined> }, ...args: string[]) =>
		sh(shArgv(join(r.dir, "scripts/release.sh"), ...args), { cwd: r.dir, env: r.env });
	const lines = (out: string) => out.split("\n");

	// A promoted stable is built by the same release.yml, as a draft, so without this step it would never become public;
	// and release-sign.sh is what refuses to publish it unsigned. Its position matters: after the run, before the site.
	test("a stable promotion signs and publishes the release after the Release run, before deploying the site", () => {
		const r = repo();
		const o = releaseSh(r, "stable", "--dry-run", "--skip-checks", "--force-soak");
		expect(o.code, o.err + o.out).toBe(0);
		const l = lines(o.out);
		const watch = l.findIndex((x) => x.startsWith("+ watch the Release run for v1.5.0"));
		const sign = l.findIndex((x) => x === "+ scripts/release-sign.sh v1.5.0");
		const deploy = l.findIndex((x) => x.includes("deploying the site") || x.includes("site deploy skipped"));
		expect(watch, o.out).toBeGreaterThan(-1);
		expect(sign, o.out).toBeGreaterThan(watch);
		expect(deploy, o.out).toBeGreaterThan(sign);
	});

	test("re-publishing an existing tag signs it too", () => {
		const r = repo();
		const o = releaseSh(r, "--publish", "v1.5.0-beta.1", "--dry-run");
		expect(o.code, o.err + o.out).toBe(0);
		const l = lines(o.out);
		const sign = l.findIndex((x) => x === "+ scripts/release-sign.sh v1.5.0-beta.1");
		expect(sign, o.out).toBeGreaterThan(l.findIndex((x) => x.startsWith("+ watch the Release run")));
	});

	// #321: the reporters are told only once the release is public (signed and published), and after the site, so a
	// failure before it never says "fixed in" about a release nobody can download.
	test("reporters are told last, after the release is signed and published and the site is deployed", () => {
		for (const args of [["stable", "--dry-run", "--skip-checks", "--force-soak"], ["--publish", "v1.5.0-beta.1", "--dry-run"]]) {
			const o = releaseSh(repo(), ...args);
			expect(o.code, o.err + o.out).toBe(0);
			const l = lines(o.out);
			const sign = l.findIndex((x) => x.startsWith("+ scripts/release-sign.sh"));
			const deploy = l.findIndex((x) => x.includes("deploying the site") || x.includes("site deploy skipped"));
			const tell = l.findIndex((x) => x.startsWith("+ bun scripts/release-notify.ts v1.5.0"));
			expect(tell, o.out).toBeGreaterThan(deploy);
			expect(deploy, o.out).toBeGreaterThan(sign);
			expect(l.filter(Boolean).at(-1), o.out).toStartWith("+ bun scripts/release-notify.ts v1.5.0");
		}
	});
});

interface Step {
	uses?: string;
	with?: Record<string, unknown>;
}
interface Job {
	needs?: string[];
	if?: string;
	permissions?: Record<string, string>;
	steps: Step[];
}

describe("release.yml signing wiring", () => {
	const wf = Bun.YAML.parse(read(".github/workflows/release.yml")) as { jobs: Record<string, Job> };
	const { publish, attest } = wf.jobs;
	const step = (job: Job, action: string) => {
		const s = job.steps.find((x) => x.uses?.startsWith(`${action}@`));
		if (!s) throw new Error(`no ${action} step`);
		return s;
	};

	test("publish creates a DRAFT for every channel: nothing is public before a maintainer signs", () => {
		expect(step(publish, "softprops/action-gh-release").with?.draft).toBe(true);
	});

	test("attest needs publish, never runs for a build-only run, and has exactly the permissions of an attestation", () => {
		expect(attest.needs).toContain("publish");
		expect(attest.if).toContain("!inputs.build_only");
		expect(attest.permissions).toEqual({ contents: "read", "id-token": "write", attestations: "write" });
	});

	test("attest attests the SHA256SUMS publish built (the very bytes the maintainer signs), with a SHA-pinned action", () => {
		const a = step(attest, "actions/attest");
		expect(a.uses).toMatch(/^actions\/attest@[0-9a-f]{40}$/);
		expect(a.with?.["subject-checksums"]).toBe("SHA256SUMS");
		const up = step(publish, "actions/upload-artifact");
		expect(up.with?.path).toBe("SHA256SUMS");
		expect(step(attest, "actions/download-artifact").with?.name).toBe(up.with?.name);
	});

	test("nothing but publish may write to the repository, and no job holds a signing key", () => {
		for (const [name, job] of Object.entries(wf.jobs)) if (name !== "publish") expect(job.permissions?.contents ?? "read", name).not.toBe("write");
		expect(read(".github/workflows/release.yml")).not.toMatch(/ssh-keygen|SIGNING_KEY|RELEASE_KEY/);
	});

	test("the draft still carries SHA256SUMS and the per-asset .sha256 files old clients read", () => {
		const files = String(step(publish, "softprops/action-gh-release").with?.files);
		expect(files).toContain("SHA256SUMS");
		expect(files).toContain("artifacts/**/*.sha256");
	});
});
