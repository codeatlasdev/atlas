// bun scripts/mobile-e2e.ts [--keep] [--device <simulator name>]
//
// Atlas Mobile end to end, on an iOS Simulator against a real `atlas hostd`: the app pairs over
// iroh, lists the fixture project, opens it, its agent and the agent's terminal
// (apps/mobile/integration_test/e2e_test.dart). Nothing touches the user's Atlas or screen:
//
// - hostd runs in its own dir under /tmp (`--dir`), with no relay (the simulator shares the Mac's
//   network), a fixture project and one agent terminal.
// - The simulator is a dedicated one (`atlas-e2e`), erased each run so no earlier pairing
//   survives in its Keychain, and booted headless: Simulator.app is never opened.
//
// Exits with the test's status. `--keep` leaves hostd, the dir and the simulator running.
import { $ } from "bun";
import {
	existsSync,
	mkdirSync,
	mkdtempSync,
	readFileSync,
	rmSync,
	writeFileSync,
} from "node:fs";
import { connect } from "node:net";
import { join } from "node:path";

const arg = (k: string, d?: string) => {
	const i = process.argv.indexOf(`--${k}`);
	return i >= 0 ? process.argv[i + 1] : d;
};
const keep = process.argv.includes("--keep");
const model = arg("device", "iPhone 15 Pro")!;
const root = join(import.meta.dir, "..");
const atlas = join(root, "target/debug/atlas");
const SIM = "atlas-e2e";

type Reply = { id?: number; ok: boolean; data?: unknown; error?: string };
type PairStart = {
	node_id: string;
	code: string;
	hostname?: string;
	addrs?: string[];
};

/** One NDJSON connection to hostd: `call` sends a request and resolves with its reply data. */
async function hostd(dir: string) {
	const token = readFileSync(join(dir, "hostd.token"), "utf8").trim();
	const sock = connect(join(dir, "hostd.sock"));
	await new Promise<void>((ok, fail) =>
		sock.once("connect", ok).once("error", fail),
	);
	const waiting = new Map<number, (v: Reply) => void>();
	let buf = "";
	sock.on("data", (d) => {
		buf += d.toString();
		for (let nl = buf.indexOf("\n"); nl >= 0; nl = buf.indexOf("\n")) {
			const m: Reply = JSON.parse(buf.slice(0, nl));
			buf = buf.slice(nl + 1);
			if (typeof m.id === "number") waiting.get(m.id)?.(m);
		}
	});
	sock.write(
		`${JSON.stringify({ hello: { token, client: "test", proto: 2 } })}\n`,
	);
	let id = 0;
	const call = (cmd: string, args: object = {}) =>
		new Promise<unknown>((ok, fail) => {
			waiting.set(++id, (m) => {
				waiting.delete(m.id ?? -1);
				if (m.ok) ok(m.data);
				else fail(new Error(`${cmd}: ${m.error}`));
			});
			sock.write(`${JSON.stringify({ id, cmd, args })}\n`);
		});
	return { call, close: () => sock.end() };
}

/** POST one hook event the way `atlas hook` does. */
async function hook(dir: string, body: object) {
	const env = Object.fromEntries(
		readFileSync(join(dir, "desktop/hook-endpoint.env"), "utf8")
			.split("\n")
			.map((l) => l.split("=", 2)),
	);
	const r = await fetch(`http://127.0.0.1:${env.ATLAS_HOOK_PORT}/hook/omp`, {
		method: "POST",
		headers: {
			"X-Atlas-Hook-Token": env.ATLAS_HOOK_TOKEN,
			"Content-Type": "application/json",
		},
		body: JSON.stringify(body),
	});
	if (!r.ok) throw new Error(`hook: HTTP ${r.status}`);
}

async function waitFor(what: string, ok: () => boolean, ms = 15_000) {
	for (const end = Date.now() + ms; Date.now() < end; await Bun.sleep(100))
		if (ok()) return;
	throw new Error(`timed out waiting for ${what}`);
}

/** The dedicated simulator's UDID, created on first use. */
async function simulator(): Promise<string> {
	const list: { devices: Record<string, { name: string; udid: string }[]> } =
		JSON.parse(await $`xcrun simctl list devices available -j`.text());
	const found = Object.values(list.devices)
		.flat()
		.find((d) => d.name === SIM);
	if (found) return found.udid;
	return (await $`xcrun simctl create ${SIM} ${model}`.text()).trim();
}

/** Fixture: a project with two services and a log line, registered where hostd reads it. */
function fixture(dir: string): string {
	const proj = join(dir, "e2e-shop");
	mkdirSync(join(proj, ".logs"), { recursive: true });
	writeFileSync(
		join(proj, "atlas.yaml"),
		"name: e2e-shop\nservices:\n  api: { command: 'true', port: 4100 }\n  worker: { command: 'true', depends_on: [api] }\n",
	);
	const now = new Date().toTimeString().slice(0, 8);
	writeFileSync(
		join(proj, ".logs/api.log"),
		`${now} [stdout] listening on 4100\n`,
	);
	mkdirSync(join(dir, "desktop"), { recursive: true });
	writeFileSync(
		join(dir, "desktop/projects.json"),
		JSON.stringify([{ id: "e2e", root: proj }]),
	);
	return proj;
}

// /tmp, not os.tmpdir(): macOS caps a unix socket path at 104 bytes.
const dir = mkdtempSync("/tmp/atlas-e2e-");
let udid = "";
let status = 1;
try {
	await $`cargo build -p atlas-cli`.cwd(root);
	const proj = fixture(dir);

	// hostd: isolated dir, no relay. It runs in the foreground; `--stop --dir` ends it.
	Bun.spawn([atlas, "hostd", "--dir", dir], {
		env: { ...process.env, ATLAS_MOBILE_RELAY: "0" },
		stdout: "ignore",
		stderr: Bun.file(join(dir, "hostd.out")),
	});
	await waitFor("hostd socket", () => existsSync(join(dir, "hostd.sock")));
	const h = await hostd(dir);
	await waitFor("hook endpoint", () => {
		try {
			return readFileSync(
				join(dir, "desktop/hook-endpoint.env"),
				"utf8",
			).includes("PORT");
		} catch {
			return false;
		}
	});

	// One agent terminal in the project: its output is the terminal replay, its hook the state.
	await h.call("pty_create", {
		cwd: proj,
		argv: ["sh", "-c", "echo hello from e2e; exec cat"],
		cols: 100,
		rows: 30,
		scope: { pane_key: "e2e-agent" },
	});
	await hook(dir, {
		pane_key: "e2e-agent",
		payload: { event: "agent_end", text: "e2e reply" },
	});

	const p = (await h.call("mobile_pair_start")) as PairStart; // hostd's mobile::pair_start shape
	h.close();
	const q = new URLSearchParams({
		node: p.node_id,
		code: p.code,
		name: p.hostname || "e2e",
	});
	for (const a of p.addrs ?? []) q.append("addr", a);
	const url = `atlas-mobile://pair?${q}`;

	udid = await simulator();
	await $`xcrun simctl shutdown ${udid}`.quiet().nothrow();
	await $`xcrun simctl erase ${udid}`;
	await $`xcrun simctl boot ${udid}`;
	await $`xcrun simctl bootstatus ${udid} -b`.quiet();

	const run =
		await $`flutter test integration_test/e2e_test.dart -d ${udid} --dart-define=E2E_PAIR_URL=${url} --dart-define=ATLAS_ALWAYS_ONBOARD=false`
			.cwd(join(root, "apps/mobile"))
			.nothrow();
	status = run.exitCode;
} catch (e) {
	console.error(`mobile-e2e: ${e instanceof Error ? e.message : e}`);
} finally {
	if (!keep) {
		await $`${atlas} hostd --stop --dir ${dir}`.quiet().nothrow();
		if (udid) await $`xcrun simctl shutdown ${udid}`.quiet().nothrow();
		rmSync(dir, { recursive: true, force: true });
	} else {
		console.log(`kept: hostd dir ${dir}, simulator ${SIM} (${udid})`);
	}
}
process.exit(status);
