// Lifecycle of one isolated Atlas Desktop capture: scratch HOME + hostd dir + seeded projects, the debug bundle
// spawned by us, the scene run against its debug bridge while winrec records its window, then teardown of exactly
// the processes we started and removal of the scratch dir (always, in `finally`).
//
// Isolation: the instance runs with its own HOME (so `~/.atlas/desktop-debug.sock`, projects.json, settings, hook
// endpoint are all separate from the user's app) and its own ATLAS_HOSTD_DIR (so it never attaches to the user's
// `atlas hostd`). We never touch a process we did not spawn or find under our scratch HOME.

import { chmodSync, copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import type { Subprocess } from "bun";
import { registerAsset } from "../lib/assets.ts";
import { run } from "../lib/exec.ts";
import { REPO, STUDIO, StudioError } from "../lib/paths.ts";
import { makeCtx, sleep, type Scene, type SceneEnv } from "./ctx.ts";
import { ensureWinrec } from "./winrec.ts";

const HERE = join(STUDIO, "src/capture");
const BUNDLE_BUILT = join(REPO, "target/debug/bundle/macos/Atlas.app/Contents/MacOS/atlas-desktop");
const ATLAS_BUILT = join(REPO, "target/debug/atlas");
/** Canonical short scratch root (macOS /tmp -> /private/tmp). */
const SCRATCH = realpathSync("/tmp");
const BUILD_HINT =
	"build it from the repo root:\n  cargo build -p atlas-cli && codesign -f -s - target/debug/atlas\n  bun run desktop:build -- --debug   (debug bundle at target/debug/bundle/macos/Atlas.app)";

/** Projects the 1.0 film's sidebar showed, in sidebar order. orbit-shop is the demo stack; the rest are one-service stacks. */
const SIDE_PROJECTS = [
	{ name: "lumen-app", port: 4200, hue: 330 },
	{ name: "nova-docs", port: 4300, hue: 95 },
	{ name: "pulse-api", port: 4400, hue: 285 },
] as const;
/** Worktrees of orbit-shop and the branch each one carries (offsets 100/200/300 are assigned by the app). */
export const ORBIT_WORKTREES = [
	{ name: "checkout-shipping", prompt: "Add shipping cost to checkout total" },
	{ name: "fix-stripe-timeout", prompt: "Retry stripe timeouts with backoff" },
	{ name: "cart-empty-state", prompt: "Design an empty cart state" },
] as const;

const git = (cwd: string, ...args: string[]) =>
	run(["git", "-c", "user.name=Atlas Studio", "-c", "user.email=studio@atlas.invalid", "-c", "commit.gpgsign=false", ...args], { cwd });

function seedProject(root: string, name: string, yaml: string) {
	mkdirSync(root, { recursive: true });
	writeFileSync(join(root, "atlas.yaml"), yaml);
	writeFileSync(join(root, "README.md"), `# ${name}\n`);
	writeFileSync(join(root, ".gitignore"), ".logs/\n");
}

function sideYaml(name: string, port: number): string {
	return `name: ${name}
services:
  app:
    command: "while true; do echo tick; sleep 1; done"
    port: ${port}
    ready_log: tick
`;
}

/** What the scene needs from a scratch run; `app` resolves paths and facts, `stop` tears down. */
interface Instance {
	env: SceneEnv;
	/** Pid of the atlas-desktop we spawned. */
	pid: number;
	tmp: string;
}

async function seed(tmp: string, atlasBin: string): Promise<SceneEnv> {
	const home = join(tmp, "home");
	const projects = join(tmp, "projects");
	mkdirSync(join(home, ".atlas/desktop"), { recursive: true });
	mkdirSync(join(tmp, "hostd"), { recursive: true });
	mkdirSync(projects, { recursive: true });
	// Dev-build look of the film ("Dev build · Updates paused"), no welcome/tour/checklist, no network update checks.
	writeFileSync(
		join(home, ".atlas/desktop/settings.json"),
		JSON.stringify({
			onboarding: { done: true, toured: true, checklist: { dismissed: true, done: [] } },
			updates: { auto: false, lastSeen: "1.0.1", notified: null, skipped: null },
			sshHosts: [],
		})
	);
	// the "Enable omp integration" banner checks for this marker in $HOME; the scratch HOME gets it so the film shows no banner
	const marker = join(home, ".omp/plugins/node_modules/atlas-omp/extensions");
	mkdirSync(marker, { recursive: true });
	writeFileSync(join(marker, "atlas-status.ts"), "// scratch marker: the capture shows the integration as enabled\n");
	const roots: Record<string, string> = {};
	const orbit = join(projects, "orbit-shop");
	// orbit-shop = demo/atlas.yaml + the quiet infra of the film's panel (database, search: memory-wavy stand-ins) that
	// migrate waits for. `payments` crash-loops by design in the demo stack.
	const assets = join(HERE, "assets");
	const infra = `  database:
    command: "python3 ${assets}/memhog.py 40 0.05 1 & while true; do echo 'checkpoint complete: wrote 42 buffers'; sleep 4; done"
    port: 54320
    ready_log: checkpoint
  search:
    command: "python3 ${assets}/memhog.py 30 0.04 2 & while true; do echo 'query \\"running shoes\\" 14ms'; sleep 2.2; done"
    depends_on: { database: log_ready }
`;
	const demo = readFileSync(join(REPO, "demo/atlas.yaml"), "utf8");
	const orbitYaml = demo.replace("services:\n", `services:\n${infra}`).replace("    oneshot: true\n", "    oneshot: true\n    depends_on: { database: log_ready }\n");
	seedProject(orbit, "orbit-shop", orbitYaml);
	roots["orbit-shop"] = orbit;
	for (const p of SIDE_PROJECTS) {
		roots[p.name] = join(projects, p.name);
		seedProject(roots[p.name]!, p.name, sideYaml(p.name, p.port));
	}
	for (const root of Object.values(roots)) {
		await git(root, "init", "-q", "-b", "main");
		await git(root, "add", "-A");
		await git(root, "commit", "-q", "-m", "initial commit");
	}
	// git worktree + the app's own bookkeeping both want canonical paths (macOS /var -> /private/var)
	for (const k of Object.keys(roots)) roots[k] = realpathSync(roots[k]!);
	return { projects: roots, home, atlasBin, assets: join(HERE, "assets"), fakeAgentDir: join(HERE, "fakeagent") };
}

async function waitFor<T>(what: string, timeoutMs: number, probe: () => Promise<T | undefined>): Promise<T> {
	const deadline = Date.now() + timeoutMs;
	for (;;) {
		const v = await probe().catch(() => undefined);
		if (v !== undefined) return v;
		if (Date.now() > deadline) throw new StudioError(`timed out waiting for ${what}`, `${timeoutMs / 1000}s`);
		await sleep(300);
	}
}

/** Reply of `atlas desktop debug info` (crates/atlas-desktop/src/debug.rs). */
interface BridgeInfo {
	pid: number;
	window_id: number;
	visible: boolean;
}

/** `atlas desktop debug info` against the isolated instance (HOME decides which socket), or undefined while it is not up. */
async function bridgeInfo(env: SceneEnv): Promise<BridgeInfo | undefined> {
	const { code, out } = await run([env.atlasBin, "desktop", "debug", "info"], { env: { HOME: env.home }, allowFail: true });
	if (code !== 0) return undefined;
	const info: BridgeInfo = JSON.parse(out); // our own CLI's output: a flat object with exactly these fields
	return info;
}

/** Pids of one scratch stack: its runner (`.logs/.tui.lock`) and the services it spawned (`.logs/.pids`). Exact pids only. */
function stackPids(root: string): number[] {
	const pids: number[] = [];
	for (const f of [".tui.lock", ".pids"]) {
		const file = join(root, ".logs", f);
		if (!existsSync(file)) continue;
		for (const word of readFileSync(file, "utf8").split(/\s+/)) {
			const pid = Number(word);
			if (Number.isInteger(pid) && pid > 1) pids.push(pid);
		}
	}
	return pids;
}

/** Stop only what we started: the stacks under our scratch (`atlas down`, then exact-pid SIGTERM of what is left), the app, our hostd. */
async function teardown(inst: Instance | null, tmp: string, appProc: Subprocess | null) {
	const atlas = join(tmp, "bin/atlas");
	const hostdDir = join(tmp, "hostd");
	const hostdPid = existsSync(join(hostdDir, "hostd.pid")) ? Number(readFileSync(join(hostdDir, "hostd.pid"), "utf8").trim()) : 0;
	if (inst) {
		for (const root of Object.values(inst.env.projects)) {
			const pids = stackPids(root); // read before `down` removes the lock
			await run([inst.env.atlasBin, "down", "--dir", root], { env: { HOME: inst.env.home }, allowFail: true });
			await sleep(300);
			// a recycled pid must not be hit: only kill what still runs from our scratch dir
			for (const pid of pids) if (await runsUnder(pid, tmp)) process.kill(pid, "SIGTERM");
		}
	}
	if (appProc) {
		appProc.kill("SIGTERM");
		await Promise.race([appProc.exited, sleep(5000)]);
		if (alive(appProc.pid)) appProc.kill("SIGKILL");
	}
	if (hostdPid > 1) {
		await run([atlas, "hostd", "--stop", "--dir", hostdDir], { env: { ATLAS_HOSTD_DIR: hostdDir }, allowFail: true });
		await sleep(500);
		if (alive(hostdPid)) process.kill(hostdPid, "SIGTERM");
	}
	await killUnder(tmp);
	// never remove anything but our own mkdtemp dir
	if (!tmp.startsWith(`${SCRATCH}/atlas-studio-`) || tmp.includes("..")) throw new StudioError(`refusing to remove ${tmp}`, "not a scratch dir of this capture");
	rmSync(tmp, { recursive: true, force: true });
}

const alive = (pid: number) => {
	try {
		process.kill(pid, 0);
		return true;
	} catch {
		return false;
	}
};

/** True when `pid` is alive and its command line mentions `dir`. */
async function runsUnder(pid: number, dir: string): Promise<boolean> {
	const { out } = await run(["ps", "-o", "command=", "-p", String(pid)], { allowFail: true });
	return out.includes(dir);
}

/** Exact-pid SIGTERM of any leftover process whose command line mentions our scratch dir (service shells, memhog…). */
async function killUnder(tmp: string) {
	const { out } = await run(["ps", "-axo", "pid=,command="], { allowFail: true });
	for (const line of out.split("\n")) {
		const m = /^\s*(\d+)\s+(.*)$/.exec(line);
		if (!m || !m[2]!.includes(tmp) || Number(m[1]) === process.pid) continue;
		try {
			process.kill(Number(m[1]), "SIGTERM");
		} catch {
			// already gone
		}
	}
}

export interface AppCaptureOpts {
	/** Manifest key to register under; default `rec:<scene>`. */
	as?: string;
}

/** Run one scene end to end and register the recording. Returns the registered key. */
export async function captureScene(scene: Scene, opts: AppCaptureOpts = {}): Promise<string> {
	for (const [what, path] of [
		["the debug bundle", BUNDLE_BUILT],
		["the atlas CLI", ATLAS_BUILT],
	] as const) {
		if (!existsSync(path)) throw new StudioError(`${what} is missing: ${path}`, BUILD_HINT);
	}
	const winrec = await ensureWinrec();
	// /tmp (= /private/tmp) keeps the project path short on screen; the Panel prints it
	const tmp = realpathSync(mkdtempSync(join(SCRATCH, "atlas-studio-")));
	let appProc: Subprocess | null = null;
	let inst: Instance | null = null;
	try {
		// private copies: a `cargo build`/`clean` running elsewhere cannot pull the binaries out from under the capture
		mkdirSync(join(tmp, "bin"));
		const atlas = join(tmp, "bin/atlas");
		const desktop = join(tmp, "bin/atlas-desktop");
		copyFileSync(ATLAS_BUILT, atlas);
		copyFileSync(BUNDLE_BUILT, desktop);
		const env = await seed(tmp, atlas);
		const fakePath = [env.fakeAgentDir, "/opt/homebrew/bin", "/usr/bin", "/bin", "/usr/sbin", "/sbin", join(process.env.HOME ?? "", ".bun/bin")].join(":");
		chmodSync(join(env.fakeAgentDir, "omp"), 0o755);
		appProc = Bun.spawn([desktop], {
			env: { HOME: env.home, ATLAS_HOSTD_DIR: join(tmp, "hostd"), ATLAS_BIN: atlas, PATH: fakePath, SHELL: "/bin/zsh", TERM: "xterm-256color", LANG: "en_US.UTF-8" },
			stdin: "ignore",
			stdout: Bun.file(join(tmp, "app.log")),
			stderr: Bun.file(join(tmp, "app.log")),
		});
		inst = { env, pid: appProc.pid, tmp };

		// the bridge answers once the window and webview exist; the stores appear after the layout's onMount
		const info = await waitFor("the debug bridge", 60_000, () => bridgeInfo(env));
		if (info.pid !== appProc.pid) throw new StudioError(`debug bridge answered for pid ${info.pid}, not our app (${appProc.pid})`, "refusing to drive a foreign instance");
		const { ctx, close } = await makeCtx(env.home, env);
		try {
			await waitFor("the app stores", 60_000, async () => {
				const ok = await ctx.eval<boolean>(`return !!window.__atlas && window.__atlas.ws.loaded && window.__atlas.store.loaded`);
				return ok ? true : undefined;
			});
			for (const root of Object.values(env.projects)) {
				await ctx.eval(`await window.__TAURI_INTERNALS__.invoke("projects_add", { root: ${JSON.stringify(root)} }); await window.__atlas.store.refresh(); return 1`);
			}
			await ctx.eval(`await window.__atlas.store.refresh(); return window.__atlas.store.projects.length`);
			await waitFor("the seeded projects in the sidebar", 20_000, async () =>
				(await ctx.eval<number>(`return window.__atlas.ws.doc.worktrees.length`)) >= Object.keys(env.projects).length ? true : undefined
			);
			await scene.setup?.(ctx);
			const wid = (await bridgeInfo(env))?.window_id;
			if (!wid) throw new StudioError("the app lost its window before recording", "see app.log in the scratch dir (removed on exit)");
			const out = join(tmp, `${scene.id}.mov`);
			const rec = Bun.spawn([winrec, String(wid), String(scene.seconds), out, "60"], { stdout: "pipe", stderr: "pipe" });
			await waitForLine(rec.stdout, "recording", rec);
			await scene.body(ctx);
			const code = await rec.exited;
			if (code !== 0) throw new StudioError(`winrec exited ${code}`, (await new Response(rec.stderr).text()).trim());
			const { out: ver } = await run([atlas, "version"]);
			const { out: sha } = await run(["git", "rev-parse", "--short", "HEAD"], { cwd: REPO });
			const key = opts.as ?? `rec:${scene.id}`;
			await registerAsset(key, "recording", out, `bun studio capture app ${scene.id}`, {
				atlasVersion: /atlas (\S+)/.exec(ver)?.[1],
				gitSha: sha.trim(),
				seconds: scene.seconds,
			});
			return key;
		} finally {
			close();
		}
	} finally {
		await teardown(inst, tmp, appProc);
	}
}

/** Resolve once the recorder prints `line` (it has started capturing); reject if it exits first. */
async function waitForLine(stdout: ReadableStream, line: string, proc: Subprocess) {
	const reader = stdout.getReader();
	let seen = "";
	const exited = proc.exited.then((code) => {
		throw new StudioError(`winrec exited ${code} before recording started`, "see its message above; Screen Recording permission is the usual cause");
	});
	const started = (async () => {
		for (;;) {
			const { value, done } = await reader.read();
			if (done) return;
			seen += new TextDecoder().decode(value);
			if (seen.includes(line)) return;
		}
	})();
	await Promise.race([started, exited]);
	reader.releaseLock();
}
