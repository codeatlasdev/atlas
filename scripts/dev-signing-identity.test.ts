import { expect, test } from "bun:test";
import { chmodSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

// Incident (2026-10-27): "it says no permission although I allowed it". The debug bundle was signed ad hoc, and macOS
// ties a privacy grant (Screen Recording, Microphone) to the app's code requirement, which for an ad-hoc signature is
// its cdhash: every rebuild silently invalidated the grant (TCC held cdhash a3a6…, the running build was bf37…). With
// an Apple Development identity in the keychain the requirement is identity + bundle id, stable across rebuilds.
const script = join(import.meta.dir, "dev-signing-identity.sh");

function run(securityOutput: string | null, env: Record<string, string> = {}) {
	const bin = mkdtempSync(join(tmpdir(), "fake-security-"));
	if (securityOutput !== null) {
		const fake = join(bin, "security");
		writeFileSync(fake, `#!/bin/sh\ncat <<'OUT'\n${securityOutput}\nOUT\n`);
		chmodSync(fake, 0o755);
	}
	const r = Bun.spawnSync(["sh", script], { env: { PATH: `${bin}:/usr/bin:/bin`, ...env } });
	return r.stdout.toString().trim();
}

test("an Apple Development identity in the keychain signs the debug bundle", () => {
	const out = `  1) 929FD4 "Apple Development: Created via API (A4FQ692QSS)"\n     1 valid identities found`;
	expect(run(out)).toBe("Apple Development: Created via API (A4FQ692QSS)");
});

test("no development identity (or no security tool): ad hoc, as before", () => {
	expect(run(`  0 valid identities found`)).toBe("-");
	expect(run(`  1) AB "Developer ID Application: X (T)"\n     1 valid identities found`)).toBe("-");
});

test("an explicit APPLE_SIGNING_IDENTITY wins", () => {
	expect(run(`  1) 92 "Apple Development: A (B)"`, { APPLE_SIGNING_IDENTITY: "Mine" })).toBe("Mine");
});
