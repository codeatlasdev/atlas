import { afterEach, describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

// Regression: the first release run with the Developer ID certificate (run 44112660069) failed in "macOS signing
// setup" with `security: SecKeychainItemImport: MAC verification failed during PKCS12 import (wrong password?)`.
// The runner's openssl is OpenSSL 3, whose default PKCS#12 (AES-256, SHA-256 MAC) `security` cannot read.
// Driven for real on macOS: openssl makes a certificate, `security` imports it into a throwaway keychain.
const script = join(import.meta.dir, "macos-keychain-import.sh");
const CN = "Developer ID Application: ATLAS KEYCHAIN TEST (ABCDE12345)";

// Every openssl a release runner or a maintainer's Mac may have first on PATH.
const openssls = ["/opt/homebrew/opt/openssl@3/bin/openssl", "/usr/local/opt/openssl@3/bin/openssl", "/usr/bin/openssl"].filter(
	(p) => existsSync(p),
);

const dirs: string[] = [];
const keychains: string[] = [];
afterEach(() => {
	for (const kc of keychains.splice(0)) Bun.spawnSync(["security", "delete-keychain", kc]);
	for (const d of dirs.splice(0)) rmSync(d, { recursive: true, force: true });
});

function sh(cmd: string[], cwd: string, env: Record<string, string> = {}) {
	const r = Bun.spawnSync(cmd, { cwd, env: { ...process.env, ...env } });
	if (r.exitCode !== 0) throw new Error(`${cmd.join(" ")}: ${r.stderr.toString()}`);
}

describe.skipIf(process.platform !== "darwin")("macos-keychain-import.sh", () => {
	for (const openssl of openssls) {
		test(`a .p12 written by ${openssl} with its defaults imports into a new keychain`, () => {
			const dir = mkdtempSync(join(tmpdir(), "kcimport-"));
			dirs.push(dir);
			sh([openssl, "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", "k.pem", "-out", "c.pem", "-days", "1", "-subj", `/CN=${CN}`], dir);
			sh(["sh", "-c", "cat c.pem k.pem > both.pem"], dir);
			// The secret as the maintainer's openssl wrote it: no algorithm flags.
			sh([openssl, "pkcs12", "-export", "-in", "both.pem", "-passout", "env:P", "-out", "in.p12"], dir, { P: "secret-pw" });
			const kc = join(dir, "test.keychain-db");
			keychains.push(kc);
			const r = Bun.spawnSync(["bash", script, join(dir, "in.p12"), kc], {
				cwd: dir,
				// This openssl first, as on the runner where Homebrew's OpenSSL 3 shadows /usr/bin/openssl.
				env: { PATH: `${dirname(openssl)}:/usr/bin:/bin`, CERT_PASSWORD: "secret-pw", KEYCHAIN_PASSWORD: "kc-pw" },
			});
			expect(r.exitCode, r.stderr.toString()).toBe(0);
			const found = Bun.spawnSync(["security", "find-certificate", "-c", CN, kc]);
			expect(found.exitCode, found.stderr.toString()).toBe(0);
		}, 30_000);
	}

	test("a wrong CERT_PASSWORD fails before any keychain exists, naming the file", () => {
		const dir = mkdtempSync(join(tmpdir(), "kcimport-"));
		dirs.push(dir);
		sh(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", "k.pem", "-out", "c.pem", "-days", "1", "-subj", `/CN=${CN}`], dir);
		sh(["sh", "-c", "cat c.pem k.pem > both.pem"], dir);
		sh(["openssl", "pkcs12", "-export", "-in", "both.pem", "-passout", "env:P", "-out", "in.p12"], dir, { P: "secret-pw" });
		const kc = join(dir, "test.keychain-db");
		const r = Bun.spawnSync(["bash", script, join(dir, "in.p12"), kc], {
			cwd: dir,
			env: { PATH: process.env.PATH ?? "", CERT_PASSWORD: "wrong", KEYCHAIN_PASSWORD: "kc-pw" },
		});
		expect(r.exitCode).not.toBe(0);
		expect(r.stderr.toString()).toContain("cannot open");
		expect(existsSync(kc)).toBe(false);
		// No password in the output.
		expect(r.stderr.toString() + r.stdout.toString()).not.toContain("kc-pw");
	});
});
