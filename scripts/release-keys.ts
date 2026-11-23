#!/usr/bin/env bun
// The release signing keys (#279) as shell scripts need them. crates/atlas-core/data/release-keys.json is the one
// list: the CLI embeds it (atlas_core::release_verify), and release-sign.sh, install.sh and install.ps1 take their
// `allowed_signers` from it, so a key is added or retired in one place.
//
//   bun scripts/release-keys.ts allowed-signers   # an OpenSSH allowed_signers file (namespace atlas-release)
//   bun scripts/release-keys.ts count             # how many keys are configured
//   bun scripts/release-keys.ts signed-since      # the first tag that must be signed ("" while unset)

export interface ReleaseKey {
	name: string;
	pubkey: string;
	valid_after?: string | null;
	valid_before?: string | null;
}
export interface ReleaseKeys {
	signed_since: string | null;
	keys: ReleaseKey[];
}

export const NAMESPACE = "atlas-release";

/** `2026-12-01` as the `20261201Z` of an allowed_signers option (UTC, valid-before exclusive). */
function day(d: string): string {
	if (!/^\d{4}-\d{2}-\d{2}$/.test(d)) throw new Error(`${d} is not a YYYY-MM-DD date`);
	return `${d.replaceAll("-", "")}Z`;
}

/** The OpenSSH allowed_signers text for `keys`: one line per key, only for the release namespace. */
export function allowedSigners(policy: ReleaseKeys): string {
	return policy.keys
		.map((k) => {
			if (!/^ssh-ed25519 \S+/.test(k.pubkey)) throw new Error(`key ${k.name}: only ssh-ed25519 public keys are accepted`);
			if (!/^[A-Za-z0-9._-]+$/.test(k.name)) throw new Error(`key name ${JSON.stringify(k.name)} must be [A-Za-z0-9._-]`);
			const opts = [`namespaces="${NAMESPACE}"`];
			if (k.valid_after) opts.push(`valid-after="${day(k.valid_after)}"`);
			if (k.valid_before) opts.push(`valid-before="${day(k.valid_before)}"`);
			const [type, blob] = k.pubkey.split(/\s+/);
			return `${k.name} ${opts.join(",")} ${type} ${blob}`;
		})
		.join("\n")
		.concat(policy.keys.length ? "\n" : "");
}

if (import.meta.main) {
	const policy = (await Bun.file(new URL("../crates/atlas-core/data/release-keys.json", import.meta.url)).json()) as ReleaseKeys;
	const cmd = process.argv[2];
	if (cmd === "allowed-signers") process.stdout.write(allowedSigners(policy));
	else if (cmd === "count") console.log(policy.keys.length);
	else if (cmd === "signed-since") console.log(policy.signed_since ?? "");
	else {
		console.error("usage: release-keys.ts allowed-signers | count | signed-since");
		process.exit(2);
	}
}
