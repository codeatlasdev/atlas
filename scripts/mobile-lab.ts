// bun run mobile:lab [--device <id|name>] [--android | --sim] [--watch] [--status] [--stop]
//
// The lab lane (docs/mobile.md, "Lab lane"): validate a branch of hostd and of the phone app next to your REAL
// Atlas, without replacing or restarting anything of it, and see your real agents.
//
//   1. builds the CLI from this checkout (target/debug/atlas, through scripts/heavy.sh);
//   2. starts (or hot-restarts) the LAB hostd from that binary: `atlas hostd --lab`, files in <~/.atlas>/lab, a
//      client of your real hostd (it mirrors its agents and forwards terminals to it; it never writes to it);
//   3. builds the "Atlas Lab" phone app (bundle id br.com.codeatlas.atlas.mobile.lab, orange icon, a "LAB · <sha>"
//      banner) and installs it on your first connected iPhone (or --device, --android, --sim) with
//      `flutter install`, beside the real app;
//   4. prints the lab's own pairing QR/URL and exits.
//
// --watch   rebuild and restart the lab hostd when crates/ change, and keep a `flutter run` (debug, hot reload) of
//           the lab app attached. Never `flutter run --release` (attached, it hangs): the one-shot path is build + install.
// --status  is the lab running, and what does it follow.
// --stop    stops the LAB hostd only. The real hostd, its terminals, ~/.atlas and the real app are never touched.
//
// Nothing here stops, restarts, upgrades or writes into your hostd or ~/.atlas (except ~/.atlas/lab), runs
// `atlas link`, or replaces the installed CLI or app.
import { $ } from "bun";
import { existsSync, readFileSync, watch as fsWatch } from "node:fs";
import { connect } from "node:net";
import { homedir } from "node:os";
import { join } from "node:path";

type Env = Record<string, string | undefined>;
export type Platform = "ios" | "android" | "sim";
export type Args = {
	mode: "run" | "status" | "stop";
	platform: Platform;
	device?: string;
	watch: boolean;
};
export type Dev = { id: string; name: string; platform: string; emulator: boolean; supported: boolean };
export type PairStart = {
	node_id: string;
	code: string;
	hostname?: string;
	addrs?: string[];
	relay_url?: string | null;
};

/** The lab's own directory: `<main hostd dir>/lab`, main being $ATLAS_HOSTD_DIR or ~/.atlas. */
export function labDir(env: Env, home: string): string {
	const main = mainDir(env, home);
	if (main.split("/").pop() === "lab") {
		throw new Error(`ATLAS_HOSTD_DIR=${main} is already the lab dir: point it at your real hostd dir (default ~/.atlas)`);
	}
	return join(main, "lab");
}

function mainDir(env: Env, home: string): string {
	return env.ATLAS_HOSTD_DIR || join(home, ".atlas");
}

/** The environment of every `atlas hostd --lab …` call: ATLAS_HOSTD_DIR names MAIN (the daemon derives `<main>/lab`). */
export function labEnv(env: Env, home: string): Env {
	return { ...env, ATLAS_HOSTD_DIR: mainDir(env, home) };
}

export function parseArgs(argv: string[]): Args {
	const a: Args = { mode: "run", platform: "ios", watch: false };
	const targets: Platform[] = [];
	for (let i = 0; i < argv.length; i++) {
		const k = argv[i];
		if (k === "--status") a.mode = "status";
		else if (k === "--stop") a.mode = "stop";
		else if (k === "--watch") a.watch = true;
		else if (k === "--android") targets.push("android");
		else if (k === "--sim") targets.push("sim");
		else if (k === "--device") {
			const v = argv[++i];
			if (!v || v.startsWith("--")) throw new Error("--device needs a device id or name");
			a.device = v;
		} else throw new Error(`unknown option ${k}`);
	}
	if (targets.length > 1) throw new Error("use one of --android, --sim");
	if (targets[0]) a.platform = targets[0];
	if (a.mode === "stop" && a.watch) throw new Error("--stop stops the lab hostd; it takes no --watch");
	return a;
}

/** The device to install on: the first connected physical iPhone, a physical Android phone, or a simulator. */
export function pickDevice(devs: Dev[], a: Args): Dev {
	const usable = devs.filter((d) => d.supported);
	if (a.device) {
		const q = a.device.toLowerCase();
		const hit = usable.find((d) => d.id.toLowerCase() === q || d.name.toLowerCase().includes(q));
		if (!hit) throw new Error(`no connected device matches "${a.device}" (flutter devices)`);
		return hit;
	}
	const hit =
		a.platform === "android"
			? usable.find((d) => d.platform.startsWith("android") && !d.emulator)
			: a.platform === "sim"
				? usable.find((d) => d.platform === "ios" && d.emulator)
				: usable.find((d) => d.platform === "ios" && !d.emulator);
	if (hit) return hit;
	throw new Error(
		a.platform === "android"
			? "no connected Android phone (plug one in, or pass --device <id>)"
			: a.platform === "sim"
				? "no booted iOS simulator (open one, or pass --device <id>)"
				: "no connected iPhone (unlock it and plug it in, pass --device <id>, or use --sim)",
	);
}

const defines = (sha: string) => [
	"--dart-define=ATLAS_LAB=1",
	`--dart-define=ATLAS_SHA=${sha}`,
	// The debug-only onboarding on every launch (home_screen.dart) would hide the paired lab computer.
	"--dart-define=ATLAS_ALWAYS_ONBOARD=false",
];

/** `flutter build ios` of the lab scheme (Xcode configurations *-lab: bundle id, name, icon). */
export const iosBuildArgs = (sha: string, simulator = false) => [
	"build",
	"ios",
	"--debug",
	...(simulator ? ["--simulator"] : []),
	"--flavor=lab",
	...defines(sha),
];

/** `flutter build apk`: the lab id comes from a gradle property, so the real app needs no flavor. */
export const androidBuildArgs = (sha: string) => [
	"build",
	"apk",
	"--debug",
	"--android-project-arg=atlas.lab=1",
	...defines(sha),
];

/** The attached debug run of `--watch` (hot reload). Never `--release`: attached, it hangs. */
export function runArgs(platform: Platform, device: string, sha: string): string[] {
	const native = platform === "android" ? ["--android-project-arg=atlas.lab=1"] : ["--flavor=lab"];
	return ["run", "--debug", "-d", device, ...native, ...defines(sha)];
}

/** `atlas-mobile://pair?…`, the URL the desktop's QR carries (apps/desktop/src/lib/settings/mobile.ts). */
export function pairUrl(p: PairStart, loopbackAlias?: string): string {
	const q = new URLSearchParams({ node: p.node_id, code: p.code, name: p.hostname || "Atlas" });
	const addrs = p.addrs ?? [];
	for (const a of addrs) q.append("addr", a);
	// An emulator does not share this computer's network: its loopback, by the emulator's alias.
	if (loopbackAlias)
		for (const a of addrs) if (a.startsWith("127.0.0.1:")) q.append("addr", a.replace("127.0.0.1", loopbackAlias));
	if (p.relay_url) q.set("relay", p.relay_url);
	return `atlas-mobile://pair?${q}`;
}

export function shortSha(out: string): string {
	return out.trim().slice(0, 7) || "nogit";
}

// ---------------------------------------------------------------------------------------------------------------
// Side effects below. Everything above is pure and covered by scripts/mobile-lab.test.ts.

const root = join(import.meta.dir, "..");
const mobile = join(root, "apps/mobile");
const atlas = join(root, "target/debug/atlas");
const heavy = join(root, "scripts/heavy.sh");

const log = (m: string) => console.log(`mobile:lab ${m}`);

async function sha(): Promise<string> {
	return shortSha(await $`git rev-parse HEAD`.cwd(root).quiet().nothrow().text());
}

/** `atlas hostd --lab <args>` of this checkout's binary, against the lab dir. */
function labHostd(args: string[]) {
	return $`${atlas} hostd --lab ${args}`.env(labEnv(process.env, homedir())).quiet().nothrow();
}

type Reply = { id?: number; ok: boolean; data?: unknown; error?: string };

/** One NDJSON connection to the LAB hostd (its own token and socket): `call` resolves with the reply data. */
async function labClient(dir: string) {
	const token = readFileSync(join(dir, "hostd.token"), "utf8").trim();
	const sock = connect(join(dir, "hostd.sock"));
	await new Promise<void>((ok, fail) => sock.once("connect", ok).once("error", fail));
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
	sock.write(`${JSON.stringify({ hello: { token, client: "lab", proto: 2 } })}\n`);
	let id = 0;
	const call = (cmd: string, args: object = {}) =>
		new Promise<unknown>((ok, fail) => {
			waiting.set(++id, (m) => (m.ok ? ok(m.data) : fail(new Error(`${cmd}: ${m.error}`))));
			sock.write(`${JSON.stringify({ id, cmd, args })}\n`);
		});
	return { call, close: () => sock.end() };
}

async function until<T>(what: string, f: () => T | Promise<T>, ms = 30_000): Promise<NonNullable<T>> {
	for (const end = Date.now() + ms; Date.now() < end; await Bun.sleep(150)) {
		try {
			const v = await f();
			if (v) return v as NonNullable<T>;
		} catch {
			// not yet
		}
	}
	throw new Error(`timed out waiting for ${what}`);
}

async function buildCli() {
	log("building the CLI (target/debug/atlas)…");
	const r = await $`${heavy} cargo build -p atlas-cli`.cwd(root).nothrow();
	if (r.exitCode !== 0) throw new Error("cargo build -p atlas-cli failed");
	// macOS kills an unsigned binary on first run.
	if (process.platform === "darwin") await $`codesign -f -s - ${atlas}`.quiet().nothrow();
}

/** Is the lab hostd up (does its socket answer)? */
export async function labInfo(dir: string): Promise<Record<string, unknown> | null> {
	if (!existsSync(join(dir, "hostd.sock"))) return null;
	try {
		const c = await labClient(dir);
		const info = (await c.call("info")) as Record<string, unknown>;
		c.close();
		return info;
	} catch {
		return null;
	}
}

/** (Re)start the lab hostd from this binary. Only the LAB daemon is ever stopped. */
export async function startLab(dir: string): Promise<void> {
	if (await labInfo(dir)) {
		log("restarting the lab hostd (the real one is untouched)…");
		await labHostd(["--stop"]);
		await until("the old lab hostd to stop", async () => !(await labInfo(dir)), 15_000);
	}
	// A real hostd must be there for the lab to follow: say so before spawning anything.
	const main = process.env.ATLAS_HOSTD_DIR || join(homedir(), ".atlas");
	if (!existsSync(join(main, "hostd.sock"))) {
		throw new Error(`your real hostd is not running (${main}): open Atlas first; the lab follows it`);
	}
	const out = Bun.file(join(dir, "hostd.out"));
	await Bun.write(join(dir, "hostd.out"), ""); // creates the lab dir
	// Detached, no stdin, own session: it outlives this script, like the rest of the daemons here.
	const child = Bun.spawn([atlas, "hostd", "--lab"], {
		env: labEnv(process.env, homedir()) as Record<string, string>,
		stdin: null,
		stdout: out,
		stderr: out,
		detached: true,
	});
	child.unref();
	await until("the lab hostd to answer", async () => labInfo(dir)).catch(() => {
		throw new Error(`the lab hostd did not start; see ${join(dir, "hostd.out")}:\n${readFileSync(join(dir, "hostd.out"), "utf8")}`);
	});
}

async function devices(): Promise<Dev[]> {
	const raw = JSON.parse(await $`flutter devices --machine`.cwd(mobile).quiet().text()) as {
		id: string;
		name: string;
		targetPlatform: string;
		emulator: boolean;
		isSupported: boolean;
	}[];
	return raw.map((d) => ({
		id: d.id,
		name: d.name,
		platform: d.targetPlatform,
		emulator: d.emulator,
		supported: d.isSupported,
	}));
}

/** Print the lab's own pairing code, as text and as a QR code in the terminal. */
async function pair(dir: string, alias?: string) {
	const c = await labClient(dir);
	// The endpoint binds in the background: ask until it published its addresses.
	const p = await until("the lab's iroh endpoint", async () => {
		const v = (await c.call("mobile_pair_start")) as PairStart;
		return v.addrs?.length ? v : null;
	});
	c.close();
	const url = pairUrl(p, alias);
	const { renderANSI } = await import("uqr");
	console.log(`\n${renderANSI(url)}`);
	console.log(`Scan it with Atlas Lab (Add a computer), or open:\n${url}`);
	console.log(`Code ${p.code} works once, for 2 minutes. The lab has its own pairing: your real phone app is untouched.`);
}

async function install(dev: Dev, a: Args, s: string) {
	const android = dev.platform.startsWith("android");
	const args = android ? androidBuildArgs(s) : iosBuildArgs(s, dev.emulator);
	log(`building Atlas Lab for ${dev.name}… (flutter ${args.slice(0, 3).join(" ")})`);
	const b = await $`${heavy} nice flutter ${args}`.cwd(mobile).nothrow();
	if (b.exitCode !== 0) throw new Error("flutter build failed");
	log(`installing on ${dev.name} (beside the real app)…`);
	const i = android
		? await $`flutter install --debug -d ${dev.id} --use-application-binary=${join(mobile, "build/app/outputs/flutter-apk/app-debug.apk")}`.cwd(mobile).nothrow()
		: await $`flutter install --debug --flavor=lab -d ${dev.id}`.cwd(mobile).nothrow();
	if (i.exitCode !== 0) throw new Error("flutter install failed");
}

async function status(dir: string) {
	const info = await labInfo(dir);
	if (!info) {
		console.log(`lab hostd: not running (${dir})`);
		return;
	}
	console.log(`lab hostd: running (pid ${info.pid}, ${info.clients} client(s)), following ${info.lab}`);
	console.log(`           files: ${dir}`);
}

async function stop(dir: string) {
	if (!(await labInfo(dir))) {
		console.log(`lab hostd: not running (${dir}); nothing to stop`);
		return;
	}
	await labHostd(["--stop"]);
	await until("the lab hostd to stop", async () => !(await labInfo(dir)), 15_000);
	console.log("lab hostd: stopped (your real hostd and app are untouched)");
}

/** --watch: rebuild + restart the lab hostd on crates/ changes; `flutter run` keeps the phone hot-reloading. */
async function watchLoop(dir: string, dev: Dev, a: Args, s: string) {
	const android = dev.platform.startsWith("android");
	log(`watching crates/ (Ctrl-C to stop); flutter run is attached to ${dev.name} for hot reload`);
	const run = Bun.spawn(["flutter", ...runArgs(android ? "android" : "ios", dev.id, s)], {
		cwd: mobile,
		stdin: "inherit",
		stdout: "inherit",
		stderr: "inherit",
	});
	let busy = false;
	let again = false;
	const rebuild = async () => {
		if (busy) return void (again = true);
		busy = true;
		try {
			do {
				again = false;
				await buildCli();
				await startLab(dir);
				log("lab hostd restarted from the new build");
			} while (again);
		} catch (e) {
			console.error(`mobile:lab ${(e as Error).message}`);
		} finally {
			busy = false;
		}
	};
	let timer: ReturnType<typeof setTimeout> | undefined;
	fsWatch(join(root, "crates"), { recursive: true }, (_ev, f) => {
		if (!f || !f.endsWith(".rs") && !f.endsWith(".toml")) return;
		clearTimeout(timer);
		timer = setTimeout(rebuild, 800);
	});
	await run.exited;
}

async function main() {
	const a = parseArgs(process.argv.slice(2));
	const dir = labDir(process.env, homedir());
	if (a.mode === "status") return status(dir);
	if (a.mode === "stop") return stop(dir);

	// Fail on the device before the heavy builds: a missing phone is the common mistake.
	const dev = pickDevice(await devices(), a);
	const s = await sha();
	await buildCli();
	await startLab(dir);
	await install(dev, a, s);
	await pair(dir, dev.emulator && dev.platform.startsWith("android") ? "10.0.2.2" : undefined);
	if (a.watch) await watchLoop(dir, dev, a, s);
}

if (import.meta.main) {
	main().then(
		() => process.exit(0),
		(e) => {
			console.error(`mobile:lab: ${(e as Error).message}`);
			process.exit(1);
		},
	);
}
