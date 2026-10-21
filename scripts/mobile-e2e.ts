// bun scripts/mobile-e2e.ts [--android] [--watch] [--record <file>] [--keep] [--device <model>]
//
// Atlas Mobile end to end, on an iOS Simulator against a real `atlas hostd`: the app pairs over
// iroh, lists the fixture project, opens it, its agent and the agent's terminal
// (apps/mobile/integration_test/e2e_test.dart). Nothing touches the user's Atlas or screen:
//
// - hostd runs in its own dir under /tmp (`--dir`), with no relay (the simulator shares the Mac's
//   network), a fixture project and one agent terminal.
// - The device is a dedicated one named `atlas-e2e`, erased each run so no earlier pairing
//   survives, and started without a window: an iOS Simulator (default) or, with `--android`, an
//   Android emulator (its own AVD; the user's AVDs are never touched). It reaches this computer's
//   loopback as 10.0.2.2.
//
// Exits with the test's status. `--keep` leaves hostd, the dir and the simulator running.
// `--watch` shows the device to follow the run live, slowed down with a banner per step (the only
// option that shows a window); `--record <file>` films its screen without showing anything.
import { $ } from "bun";
import {
	existsSync,
	readdirSync,
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
const watch = process.argv.includes("--watch");
const record = arg("record");
const android = process.argv.includes("--android");
const model = arg("device", android ? "pixel_7" : "iPhone 15 Pro")!;
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

/** What the run needs from a device: start it clean, show it, film it, stop it. */
type Device = {
	/** Start it erased; resolves with the id `flutter test -d` takes. */
	boot(): Promise<string>;
	/** A line of `flutter test` output meaning the app is built and about to run. */
	built: string;
	show(): Promise<void>;
	/** Start filming to `file`; the returned function stops and saves it. */
	film(file: string): () => Promise<void>;
	stop(): Promise<void>;
	/** How the device reaches this computer's loopback, for the pairing addresses. */
	loopback?: string;
};

function ios(): Device {
	let udid = "";
	return {
		async boot() {
			const list: {
				devices: Record<string, { name: string; udid: string }[]>;
			} = JSON.parse(await $`xcrun simctl list devices available -j`.text());
			udid =
				Object.values(list.devices)
					.flat()
					.find((d) => d.name === SIM)?.udid ??
				(await $`xcrun simctl create ${SIM} ${model}`.text()).trim();
			await $`xcrun simctl shutdown ${udid}`.quiet().nothrow();
			await $`xcrun simctl erase ${udid}`;
			await $`xcrun simctl boot ${udid}`;
			await $`xcrun simctl bootstatus ${udid} -b`.quiet();
			return udid;
		},
		built: "Xcode build done",
		async show() {
			await $`open -a Simulator --args -CurrentDeviceUDID ${udid}`;
		},
		film(file) {
			// Stopped with SIGINT so the file is finalized.
			const p = Bun.spawn(
				["xcrun", "simctl", "io", udid, "recordVideo", "--force", file],
				{
					stdout: "ignore",
					stderr: "ignore",
				},
			);
			return async () => {
				p.kill("SIGINT");
				await p.exited;
			};
		},
		async stop() {
			await device.stop();
		},
	};
}

function androidEmulator(): Device {
	const sdk =
		process.env.ANDROID_HOME ??
		join(process.env.HOME ?? "", "Library/Android/sdk");
	const adb = join(sdk, "platform-tools/adb");
	// A fixed console port: the serial is known and never one of the user's emulators.
	const serial = "emulator-5584";
	let emu: ReturnType<typeof Bun.spawn> | null = null;
	const sh = (...a: string[]) => $`${adb} -s ${serial} ${a}`.quiet().nothrow();
	return {
		async boot() {
			const avds = (
				await $`${join(sdk, "emulator/emulator")} -list-avds`.text()
			).split("\n");
			if (!avds.includes(SIM)) {
				// Newest arm64 Google APIs image installed. The AVD is written directly (two small
				// files, as Android Studio writes them): avdmanager lags the SDK's image metadata.
				const image = readdirSync(join(sdk, "system-images"))
					.filter((api) => /^android-\d+$/.test(api))
					.sort((a, b) => Number(b.slice(8)) - Number(a.slice(8)))
					.flatMap((api) =>
						readdirSync(join(sdk, "system-images", api)).map((tag) => ({
							api,
							tag,
						})),
					)
					.find(
						({ api, tag }) =>
							tag.startsWith("google_apis") &&
							// A partial download has no kernel and cannot boot.
							existsSync(
								join(sdk, "system-images", api, tag, "arm64-v8a/kernel-ranchu"),
							),
					);
				if (!image)
					throw new Error(
						"no arm64 Google APIs system image in the Android SDK",
					);
				const avd = join(process.env.HOME ?? "", ".android/avd");
				mkdirSync(join(avd, `${SIM}.avd`), { recursive: true });
				writeFileSync(
					join(avd, `${SIM}.ini`),
					`avd.ini.encoding=UTF-8\npath=${join(avd, `${SIM}.avd`)}\ntarget=${image.api}\n`,
				);
				writeFileSync(
					join(avd, `${SIM}.avd/config.ini`),
					[
						"avd.ini.encoding=UTF-8",
						"abi.type=arm64-v8a",
						"hw.cpu.arch=arm64",
						"hw.cpu.ncore=4",
						"hw.ramSize=2048",
						"disk.dataPartition.size=4G",
						"hw.gpu.enabled=yes",
						"hw.gpu.mode=auto",
						"hw.keyboard=yes",
						"hw.lcd.density=420",
						"hw.lcd.width=1080",
						"hw.lcd.height=2400",
						"hw.device.name=pixel_7",
						`image.sysdir.1=system-images/${image.api}/${image.tag}/arm64-v8a/`,
						`tag.id=${image.tag}`,
						`target=${image.api}`,
						"",
					].join("\n"),
				);
			}
			// No window unless watching; -wipe-data: no pairing survives from a previous run.
			emu = Bun.spawn(
				[
					join(sdk, "emulator/emulator"),
					"-avd",
					SIM,
					"-port",
					"5584",
					"-wipe-data",
					"-no-snapshot",
					"-no-audio",
					"-no-boot-anim",
					...(watch ? [] : ["-no-window"]),
				],
				{ stdout: "ignore", stderr: "ignore" },
			);
			// Never wait on an emulator that already quit (a broken image exits at once).
			let quit = false;
			void emu.exited.then(() => {
				quit = true;
			});
			for (
				const end = Date.now() + 180_000;
				Date.now() < end;
				await Bun.sleep(1000)
			) {
				if (quit)
					throw new Error(
						`the Android emulator exited at start (try: emulator -avd ${SIM})`,
					);
				const booted = await sh("shell", "getprop", "sys.boot_completed");
				if (booted.text().trim() === "1") return serial;
			}
			throw new Error("the Android emulator did not boot in 3 minutes");
		},
		built: "Built build/app/outputs",
		// The window is already up when watching: an emulator cannot be shown after a headless start.
		async show() {},
		film(file) {
			// screenrecord stops by itself at 3 minutes, longer than a run.
			const p = Bun.spawn(
				[adb, "-s", serial, "shell", "screenrecord", "/sdcard/e2e.mp4"],
				{
					stdout: "ignore",
					stderr: "ignore",
				},
			);
			return async () => {
				p.kill("SIGINT");
				await p.exited;
				await Bun.sleep(1000);
				await $`${adb} -s ${serial} pull /sdcard/e2e.mp4 ${file}`.quiet();
			};
		},
		async stop() {
			if (!emu) return;
			await sh("emu", "kill");
			await emu.exited;
		},
		loopback: "10.0.2.2",
	};
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
const device = android ? androidEmulator() : ios();
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
	// An emulator does not share this computer's network: its loopback, by the emulator's alias.
	if (device.loopback)
		for (const a of p.addrs ?? [])
			if (a.startsWith("127.0.0.1:"))
				q.append("addr", a.replace("127.0.0.1", device.loopback));
	const url = `atlas-mobile://pair?${q}`;

	const id = await device.boot();
	const saveFilm = record ? device.film(record) : null;

	const defines = [
		`--dart-define=E2E_PAIR_URL=${url}`,
		"--dart-define=ATLAS_ALWAYS_ONBOARD=false",
	];
	if (watch) defines.push("--dart-define=E2E_SLOW=true");
	const test = Bun.spawn(
		// A stuck test fails with a message instead of running on (the build is not counted).
		[
			"flutter",
			"test",
			"integration_test/e2e_test.dart",
			"-d",
			id,
			"--timeout",
			"5m",
			...defines,
		],
		{ cwd: join(root, "apps/mobile"), stdout: "pipe", stderr: "inherit" },
	);
	// --watch: show the device once the app is built, so the window shows the run, not
	// minutes of home screen while it compiles.
	let shown = !watch;
	const out = test.stdout.pipeThrough(new TextDecoderStream()).getReader();
	for (let c = await out.read(); !c.done; c = await out.read()) {
		process.stdout.write(c.value);
		if (!shown && c.value.includes(device.built)) {
			shown = true;
			await device.show();
		}
	}
	const run = { exitCode: await test.exited };
	status = run.exitCode;
	if (saveFilm) {
		await saveFilm();
		console.log(`recording: ${record}`);
	}
} catch (e) {
	// A failed shell command carries its stderr: print it, not just the exit code.
	const err = e as { message?: string; stderr?: { toString(): string } };
	console.error(
		`mobile-e2e: ${err.message ?? e}\n${err.stderr?.toString() ?? ""}`,
	);
} finally {
	if (!keep) {
		await $`${atlas} hostd --stop --dir ${dir}`.quiet().nothrow();
		await device.stop();
		rmSync(dir, { recursive: true, force: true });
	} else {
		console.log(`kept: hostd dir ${dir}, device ${SIM}`);
	}
}
process.exit(status);
