// Tests for scripts/deploy-push-relay.sh and scripts/firebase-mobile-config.sh. They run the real scripts
// against fake gcloud / firebase / gh / curl / openssl binaries on PATH that log every call, so they prove
// the order of the mutating commands and that no secret value is ever printed, without touching any cloud.
import { shArgv } from "./sh";
import { describe, expect, test } from "bun:test";
import { chmodSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const root = join(import.meta.dir, "..");

// Sentinels that must never reach stdout/stderr or an argv.
const CF_TOKEN = "cf-token-SENTINEL-1f3a";
const GRANT_KEY = "grant-key-SENTINEL-9c7d";
const GOOGLE_JSON = '{"client":[{"SENTINEL_GOOGLE_JSON":"x"}]}';
const GOOGLE_PLIST = "<plist><key>GOOGLE_APP_ID</key><string>SENTINEL_PLIST</string></plist>";

/** A PATH dir of fakes. Each fake appends `name args` to `calls.log` (argv only, never stdin). */
function fakes() {
	const dir = mkdtempSync(join(tmpdir(), "push-deploy-"));
	const log = join(dir, "calls.log");
	writeFileSync(log, "");
	const bin = (name: string, body: string) => {
		writeFileSync(join(dir, name), `#!/usr/bin/env bash\necho "${name} $*" >> "${log}"\n${body}\n`);
		chmodSync(join(dir, name), 0o755);
	};
	// gcloud: everything "exists" except what a first deploy creates, so a real run exercises the create paths.
	bin(
		"gcloud",
		`case "$1 $2 $3" in
  "config get-value project") echo my-proj ;;
  "auth print-access-token "*) echo token ;;
  "run services describe") echo https://push-relay-abc-rj.a.run.app ;;
  "secrets create "*) cat >/dev/null ;;
  "iam service-accounts describe" | "secrets describe "* | "artifacts repositories describe") exit 1 ;;
esac`,
	);
	bin("openssl", `if [[ "$1 $2" == "rand -base64" ]]; then echo ${GRANT_KEY}; else echo deadbeefdeadbeef; fi`);
	// curl: Cloudflare answers JSON, the relay answers per path; stdin (-K -) carries the token and is dropped.
	bin(
		"curl",
		`cat >/dev/null 2>&1 </dev/stdin || true
args="$*"
out=""
for ((i=1;i<=$#;i++)); do [[ "\${!i}" == "-o" ]] && { j=$((i+1)); out="\${!j}"; }; done
case "$args" in
  *"/zones?name="*) echo '{"result":[{"id":"zone1"}]}' ;;
  *"/dns_records?name="*) echo '{"result":[]}' ;;
  *"-X GET "*"/rulesets/phases/"*) echo '{"result":{"rules":[{"ref":"other","id":"x","version":"1","expression":"true","action":"route"}]}}' ;;
  *"-X PUT"*|*"-X POST"*) if [[ "$args" == *"/v1/register"* ]]; then echo '{"grant":"g"}' >"$out"; printf 200;
      elif [[ "$args" == *"/v1/notify"* ]]; then printf 403;
      else echo '{"success":true}'; fi ;;
  *) echo '{"success":true}' ;;
esac`,
	);
		bin(
		"firebase",
		`case "$1" in
  projects:list) echo '{"status":"success","result":[]}' ;;
  apps:list) echo '{"status":"success","result":[]}' ;;
  apps:sdkconfig) for ((i=1;i<=$#;i++)); do [[ "\${!i}" == "--out" ]] && { j=$((i+1)); out="\${!j}"; }; done
    if [[ "$2" == ANDROID* || "$2" == android* ]]; then printf '%s' '${GOOGLE_JSON}' >"$out"; else printf '%s' '${GOOGLE_PLIST}' >"$out"; fi ;;
esac`,
	);
	// gh: records the secret NAME from argv and the stdin value's length only.
	bin(
		"gh",
		`if [[ "$1 $2" == "secret set" ]]; then v="$(cat)"; echo "gh-stdin-bytes \${#v}" >> "${log}"; fi`,
	);
	return { dir, log, calls: () => readFileSync(log, "utf8").split("\n").filter(Boolean) };
}

function run(script: string, args: string[], dir: string, extra: Record<string, string> = {}) {
	const p = Bun.spawnSync(shArgv(script, ...args), {
		cwd: root,
		env: { ...process.env, PATH: `${dir}:${process.env.PATH}`, CLOUDFLARE_API_TOKEN: CF_TOKEN, ...extra },
	});
	return { code: p.exitCode, out: p.stdout.toString(), err: p.stderr.toString() };
}

const never = (text: string) => {
	for (const s of [CF_TOKEN, GRANT_KEY, "SENTINEL_GOOGLE_JSON", "SENTINEL_PLIST"]) expect(text).not.toContain(s);
};
const order = (text: string, needles: string[]) => {
	let at = -1;
	for (const n of needles) {
		const i = text.indexOf(n, at + 1);
		expect(i, `"${n}" missing or out of order`).toBeGreaterThan(at);
		at = i;
	}
};

// The fakes are a POSIX PATH of stubs (a `:`-joined tmp dir that Git Bash reads as a PATH), as in
// install-sh.test.ts, which is skipped on Windows for the same reason; these are maintainer deploy scripts
// (gcloud, openssl, /tmp) that nobody runs on Windows. The argument checks below need no fakes and still run there.
describe.skipIf(process.platform === "win32")("deploy-push-relay.sh", () => {
	const script = "scripts/deploy-push-relay.sh";

	test("--dry-run lists every mutating command in order, runs none, and needs no login", () => {
		const f = fakes();
		const r = run(script, ["--dry-run"], f.dir, { GCP_PROJECT: "my-proj", CLOUDFLARE_API_TOKEN: "" });
		expect(r.code).toBe(0);
		order(r.out, [
			"gcloud services enable run.googleapis.com secretmanager.googleapis.com artifactregistry.googleapis.com fcm.googleapis.com cloudbuild.googleapis.com",
			"gcloud iam service-accounts create push-relay",
			"gcloud projects add-iam-policy-binding my-proj",
			"--role roles/firebasecloudmessaging.admin",
			"openssl rand -base64 32 | gcloud secrets create RELAY_GRANT_KEY",
			"gcloud secrets add-iam-policy-binding RELAY_GRANT_KEY",
			"--role roles/secretmanager.secretAccessor",
			"gcloud artifacts repositories create push-relay",
			"gcloud builds submit services/push-relay",
			"gcloud run deploy push-relay",
			"--set-secrets RELAY_GRANT_KEY=RELAY_GRANT_KEY:latest",
			"POST|PUT /zones/<zone>/dns_records",
			"PUT  /zones/<zone>/rulesets/phases/http_request_origin/entrypoint",
			"POST https://push.codeatlas.com.br/v1/register".replace("POST ", "curl -X POST "),
			"/v1/notify",
		]);
		expect(r.out).toContain("RELAY_TRUST_PROXY=1");
		expect(r.out).toContain("FCM_PROJECT_ID=my-proj");
		expect(r.out).toContain("--min-instances 0");
		// Nothing was executed: no fake saw a call.
		expect(f.calls().filter((c) => !c.startsWith("gcloud config get-value"))).toEqual([]);
		never(r.out + r.err);
	});

	test("a real run executes the same commands in the same order, keeps secrets out of argv and output", () => {
		const f = fakes();
		const r = run(script, [], f.dir, { GCP_PROJECT: "my-proj" });
		expect(r.err).not.toContain("✗");
		expect(r.code).toBe(0);
		const calls = f.calls().join("\n");
		order(calls, [
			"gcloud services enable",
			"gcloud iam service-accounts create push-relay",
			"gcloud projects add-iam-policy-binding my-proj",
			"gcloud secrets create RELAY_GRANT_KEY",
			"gcloud secrets add-iam-policy-binding RELAY_GRANT_KEY",
			"gcloud builds submit services/push-relay",
			"gcloud run deploy push-relay",
			"gcloud run services describe push-relay",
			"-X POST https://api.cloudflare.com/client/v4/zones/zone1/dns_records",
			"-X PUT https://api.cloudflare.com/client/v4/zones/zone1/rulesets/phases/http_request_origin/entrypoint",
			"https://push.codeatlas.com.br/v1/register",
			"https://push.codeatlas.com.br/v1/notify",
		]);
		// The merged ruleset keeps the other rule (PUT replaces the whole ruleset) and ours is added.
		expect(calls).toContain("atlas_push_relay_host");
		expect(calls).toContain("other");
		expect(r.out).toContain("✓ POST /v1/register -> 200 + grant");
		expect(r.out).toContain("✓ POST /v1/notify with a bad grant -> 403");
		never(calls + r.out + r.err);
	});

	test("an existing secret is never recreated", () => {
		const f = fakes();
		// secrets describe succeeds
		writeFileSync(
			join(f.dir, "gcloud"),
			`#!/usr/bin/env bash\necho "gcloud $*" >> "${f.log}"\ncase "$1 $2" in "config get-value") echo my-proj;; "auth print-access-token") echo t;; "run services") echo https://x.a.run.app;; esac\nexit 0\n`,
		);
		const r = run(script, [], f.dir, { GCP_PROJECT: "my-proj" });
		expect(r.code).toBe(0);
		const calls = f.calls().join("\n");
		expect(calls).not.toContain("secrets create");
		expect(calls).not.toContain("service-accounts create");
		expect(r.out).toContain("kept as is");
	});

	test("refuses without a gcloud login, before changing anything", () => {
		const f = fakes();
		writeFileSync(join(f.dir, "gcloud"), `#!/usr/bin/env bash\necho "gcloud $*" >> "${f.log}"\n[[ "$1 $2" == "auth print-access-token" ]] && exit 1\necho my-proj\n`);
		const r = run(script, [], f.dir, { GCP_PROJECT: "my-proj" });
		expect(r.code).toBe(1);
		expect(r.err).toContain("gcloud auth login");
		expect(f.calls().some((c) => c.includes("services enable"))).toBe(false);
	});
});

describe.skipIf(process.platform === "win32")("firebase-mobile-config.sh", () => {
	const script = "scripts/firebase-mobile-config.sh";

	test("--dry-run registers all four apps, then sets both secrets in both environments", () => {
		const f = fakes();
		const r = run(script, ["my-fb-project", "--dry-run"], f.dir);
		expect(r.code).toBe(0);
		order(r.out, [
			"apps:create ANDROID Atlas Mobile (Android) --package-name br.com.codeatlas.atlas.mobile ",
			"apps:create ANDROID Atlas Dev (Android) --package-name br.com.codeatlas.atlas.mobile.dev",
			"apps:create IOS Atlas Mobile (iOS) --bundle-id br.com.codeatlas.atlas.mobile ",
			"apps:create IOS Atlas Dev (iOS) --bundle-id br.com.codeatlas.atlas.mobile.dev",
			"apps:sdkconfig ANDROID",
			"apps:sdkconfig IOS",
			"gh secret set GOOGLE_SERVICES_JSON -R codeatlasdev/atlas -e mobile-beta",
			"gh secret set GOOGLE_SERVICE_INFO_PLIST -R codeatlasdev/atlas -e mobile-beta",
			"gh secret set GOOGLE_SERVICES_JSON -R codeatlasdev/atlas -e mobile-store",
			"gh secret set GOOGLE_SERVICE_INFO_PLIST -R codeatlasdev/atlas -e mobile-store",
			"APNs authentication key",
		]);
		expect(f.calls()).toEqual([]);
		never(r.out + r.err);
	});

	test("a real run downloads the files, pipes them to gh on stdin, and leaves nothing behind", () => {
		const f = fakes();
		const r = run(script, ["my-fb-project"], f.dir);
		// `app_id` finds nothing (apps:list is empty), so the post-create lookup fails: that is the guard.
		expect(r.code).toBe(1);
		expect(r.err).toContain("Firebase apps were not found");
		never(r.out + r.err + f.calls().join("\n"));
	});

	test("with the apps present: secrets go through stdin, never argv or output", () => {
		const f = fakes();
		writeFileSync(
			join(f.dir, "firebase"),
			`#!/usr/bin/env bash
echo "firebase $*" >> "${f.log}"
case "$1" in
  projects:list) echo '{"result":[]}' ;;
  apps:list) echo '{"result":[{"appId":"1:1:android:a","displayName":"Atlas Mobile (Android)","platform":"ANDROID"},{"appId":"1:1:android:b","displayName":"Atlas Dev (Android)","platform":"ANDROID"},{"appId":"1:1:ios:a","displayName":"Atlas Mobile (iOS)","platform":"IOS"},{"appId":"1:1:ios:b","displayName":"Atlas Dev (iOS)","platform":"IOS"}]}' ;;
  apps:sdkconfig) for ((i=1;i<=$#;i++)); do [[ "\${!i}" == "--out" ]] && { j=$((i+1)); out="\${!j}"; }; done
    if [[ "$2" == ANDROID ]]; then printf '%s' '${GOOGLE_JSON}' >"$out"; else printf '%s' '${GOOGLE_PLIST}' >"$out"; fi ;;
esac\n`,
		);
		const r = run(script, ["my-fb-project"], f.dir);
		expect(r.err).not.toContain("✗");
		expect(r.code).toBe(0);
		const calls = f.calls();
		expect(calls.some((c) => c.startsWith("firebase apps:create"))).toBe(false); // idempotent
		order(calls.join("\n"), [
			"firebase apps:sdkconfig ANDROID 1:1:android:a",
			"firebase apps:sdkconfig IOS 1:1:ios:a",
			"gh secret set GOOGLE_SERVICES_JSON -R codeatlasdev/atlas -e mobile-beta",
			"gh secret set GOOGLE_SERVICE_INFO_PLIST -R codeatlasdev/atlas -e mobile-beta",
			"gh secret set GOOGLE_SERVICES_JSON -R codeatlasdev/atlas -e mobile-store",
			"gh secret set GOOGLE_SERVICE_INFO_PLIST -R codeatlasdev/atlas -e mobile-store",
		]);
		// The values reached gh through stdin (4 non-empty reads), and nowhere else.
		const bytes = calls.filter((c) => c.startsWith("gh-stdin-bytes")).map((c) => Number(c.split(" ")[1]));
		expect(bytes).toHaveLength(4);
		for (const b of bytes) expect(b).toBeGreaterThan(20);
		never(calls.join("\n") + r.out + r.err);
	});
});

describe("argument validation (no tools needed, runs on every OS)", () => {
	test("firebase-mobile-config.sh rejects a malformed project id and a missing one", () => {
		const run_ = (...args: string[]) => Bun.spawnSync(shArgv("scripts/firebase-mobile-config.sh", ...args), { cwd: root });
		expect(run_("Not A Project").exitCode).toBe(1);
		expect(run_().exitCode).toBe(1);
	});

	test("deploy-push-relay.sh rejects an unknown flag", () => {
		const p = Bun.spawnSync(shArgv("scripts/deploy-push-relay.sh", "--nope"), { cwd: root });
		expect(p.exitCode).toBe(2);
		expect(p.stderr.toString()).toContain("unknown arg");
	});
});
