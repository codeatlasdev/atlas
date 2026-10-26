// bun scripts/term-flood-bench.ts [--name flood] [--sync 1|0] [--hz 60] [--seconds 8] [--slow 0] [--keep] [--out /tmp/term-flood]
//
// Flicker under an agent-style redraw flood, measured in a real agent instance (never the user's app): opens a
// terminal running `scripts/term-flood.ts`, films it at 60 fps with the in-app recorder, and reports
//   - frames the renderer presented vs frames the program wrote (`term_stats.frames`): one per display refresh at
//     most, never one per pty read,
//   - blips / blank frames / flashing of the recording, judged by `flicker-sweep.ts --judge` (the app's own detector).
//   - torn samples: `term_text` (the VT's visible screen, exactly what the renderer snapshots) is read ~200 times a
//     second while the flood runs; a sample whose block rows disagree on the frame number, or has erased rows, is a
//     half-drawn frame that a draw at that instant would have put on screen.
// Exits 1 when the recording has a finding or --max-torn is exceeded (default: any torn sample on a --sync 1 flood). Run alone through `scripts/heavy.sh`; the instance is stopped at the end.
import { $ } from "bun";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { sigLoop, tally, type Signature } from "./term-flood-judge.ts";

const arg = (k: string, d: string) => {
	const i = process.argv.indexOf(`--${k}`);
	return i >= 0 ? process.argv[i + 1]! : d;
};
const name = arg("name", "flood");
const out = arg("out", "/tmp/term-flood");
const gen = [
	"--sync",
	arg("sync", "1"),
	"--hz",
	arg("hz", "60"),
	"--slow",
	arg("slow", "0"),
	"--seconds",
	String(Number(arg("seconds", "8")) + 4),
];
const seconds = Number(arg("seconds", "8"));
const root = join(import.meta.dir, "..");
const atlas = join(root, "target/debug/atlas");
const sock = `/private/tmp/atlas-agent-${name}/debug.sock`;
const env = { ...process.env, ATLAS_DEBUG_SOCK: sock };
const debug = async (...a: string[]) =>
	(await $`${atlas} desktop debug ${a}`.env(env).quiet()).text();
const evalJs = async <T = unknown>(js: string): Promise<T> =>
	JSON.parse(await debug("eval", js));
const stats = () =>
	evalJs<
		Record<
			string,
			{
				frames: number;
				interval_p50_us: number;
				interval_p95_us: number;
				work_p95_us: number;
			}
		>
	>(`return await window.__TAURI_INTERNALS__.invoke("term_stats")`);

async function main() {
	mkdirSync(out, { recursive: true });
	const file = join(
		out,
		`flood-sync${arg("sync", "1")}-slow${arg("slow", "0")}.mov`,
	);
	const log = join(out, "written.txt");
	await $`bun run desktop:agent -- --name ${name} --scratch`.cwd(root).quiet();
	try {
		const fx = "/private/tmp/term-flood-fixture";
		mkdirSync(fx, { recursive: true });
		writeFileSync(
			join(fx, "atlas.yaml"),
			'name: flood\nservices:\n  web:\n    command: "sleep 100000"\n    port: 20997\n',
		);
		await evalJs(
			`await window.__TAURI_INTERNALS__.invoke("projects_add", { root: ${JSON.stringify(fx)} }); await window.__atlas.store.refresh(); return 1`,
		);
		const wt = await evalJs<string | null>(
			`for (let i = 0; i < 100; i++) { const w = window.__atlas.ws.doc.worktrees.find((x) => x.path === ${JSON.stringify(fx)} || x.path === ${JSON.stringify(fx.replace(/^\/private/, ""))}); if (w) { window.__atlas.shell.select(w.id); return w.id; } await new Promise((r) => setTimeout(r, 100)); } return null`,
		);
		if (!wt) throw new Error("no worktree for the flood terminal");
		const argv = JSON.stringify([
			"bun",
			join(root, "scripts/term-flood.ts"),
			...gen,
			"--log",
			log,
		]);
		const tab = await evalJs<string>(
			`const t = await window.__atlas.ws.newTerminal(${JSON.stringify(wt)}, { argv: ${argv}, label: "flood" }); window.__atlas.ws.activate(t.id); return t.id`,
		);
		for (let i = 0; i < 100 && !(tab in (await stats())); i++)
			await Bun.sleep(100);
		if (!(tab in (await stats())))
			throw new Error("the flood terminal never opened its native view");
		await Bun.sleep(1500);
		const before = (await stats())[tab]!;
		// page side: read the VT screen in a loop (the signatures are classified here, by the tested oracle)
		await evalJs(sigLoop(tab));
		const rec =
			$`${atlas} desktop debug record ${file} --seconds ${String(seconds)} --fps 60`
				.env(env)
				.quiet()
				.nothrow();
		await Bun.sleep(seconds * 1000 + 800);
		await rec;
		const after = (await stats())[tab]!;
		const sigs = await evalJs<Signature[]>(
			`window.__sigs.stop = true; return window.__sigs.list`,
		);
		const t = tally(sigs);
		if (!sigs.length)
			throw new Error(
				"the page recorded no samples of the screen: the oracle is blind",
			);
		const rowHist: Record<string, number> = {};
		for (const x of sigs)
			rowHist[`${x.ids.length}id/${x.rows}rows`] =
				(rowHist[`${x.ids.length}id/${x.rows}rows`] ?? 0) + 1;
		const torn = {
			samples: sigs.length - t["before-first-frame"],
			whole: t.whole,
			torn: t.torn,
			erased: t.erased,
		};
		await Bun.sleep(4000); // the generator ends and writes its count
		const written = Number(readFileSync(log, "utf8").trim()) || 0;
		const presented = after.frames - before.frames;
		const judged =
			await $`bun ${join(root, "scripts/flicker-sweep.ts")} --judge ${file} --surface flood`
				.cwd(root)
				.nothrow()
				.quiet();
		const verdict = judged.exitCode;
		console.log(
			JSON.stringify(
				{
					seconds,
					presented,
					rowHistogram: rowHist,
					samples: torn.samples,
					whole: torn.whole,
					tornSamples: torn.torn,
					erasedSamples: torn.erased,
					presentedPerSecond: +(presented / seconds).toFixed(1),
					writtenTotal: written,
					intervalP50Ms: +(after.interval_p50_us / 1000).toFixed(1),
					intervalP95Ms: +(after.interval_p95_us / 1000).toFixed(1),
					flicker: judged.stdout.toString().trim()
						? JSON.parse(judged.stdout.toString())
						: null,
					judgeExit: verdict,
				},
				null,
				2,
			),
		);
		const tornBad =
			arg("sync", "1") === "1" &&
			torn.torn + torn.erased > Number(arg("max-torn", "0"));
		if (tornBad)
			console.error(
				`${torn.torn} of ${torn.samples} VT samples showed a half-drawn or erased frame under synchronized output`,
			);
		if (verdict || tornBad) {
			console.error(judged.stderr.toString());
			process.exit(1);
		}
	} finally {
		if (!process.argv.includes("--keep"))
			await $`bun run desktop:agent -- --stop --name ${name}`
				.cwd(root)
				.quiet()
				.nothrow();
	}
}

await main();
