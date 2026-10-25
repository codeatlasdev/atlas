// `bun run mobile:lab` touches the maintainer's real machine: his phone, his hostd dir, his Atlas. These tests pin
// the decisions that keep it away from the real installation (the pure core of scripts/mobile-lab.ts); the lab
// daemon itself is covered by crates/atlas-host (lab::tests) and crates/atlas-cli/tests/hostd.rs.
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import {
	type Dev,
	androidBuildArgs,
	iosBuildArgs,
	labDir,
	labEnv,
	pairUrl,
	parseArgs,
	pickDevice,
	runArgs,
	shortSha,
} from "./mobile-lab.ts";

describe("labDir", () => {
	test("is <main>/lab, with main from ATLAS_HOSTD_DIR or ~/.atlas", () => {
		expect(labDir({ ATLAS_HOSTD_DIR: "/x/atlas" }, "/home/u")).toBe("/x/atlas/lab");
		expect(labDir({}, "/home/u")).toBe("/home/u/.atlas/lab");
	});

	// An inherited ATLAS_HOSTD_DIR pointing at the lab itself would nest lab/lab: refuse to guess.
	test("refuses a main dir that is already a lab dir", () => {
		expect(() => labDir({ ATLAS_HOSTD_DIR: "/h/.atlas/lab" }, "/h")).toThrow(/already the lab/);
	});
});

describe("labEnv", () => {
	// The daemon reads ATLAS_HOSTD_DIR for MAIN's dir and puts its own files in <main>/lab: the env the script
	// hands the CLI must name main, never the lab, or `--lab` would nest a second `lab/lab`.
	test("keeps ATLAS_HOSTD_DIR on main and never leaks a lab-only override", () => {
		const e = labEnv({ ATLAS_HOSTD_DIR: "/m", PATH: "/bin" }, "/home/u");
		expect(e.ATLAS_HOSTD_DIR).toBe("/m");
		expect(e.PATH).toBe("/bin");
		expect(labEnv({ PATH: "/bin" }, "/home/u").ATLAS_HOSTD_DIR).toBe("/home/u/.atlas");
	});
});

describe("parseArgs", () => {
	test("defaults: build, install on the first connected iPhone, pair, exit", () => {
		expect(parseArgs([])).toMatchObject({ mode: "run", platform: "ios", watch: false });
	});
	test("--status and --stop are their own modes and take no build", () => {
		expect(parseArgs(["--status"]).mode).toBe("status");
		expect(parseArgs(["--stop"]).mode).toBe("stop");
		expect(() => parseArgs(["--stop", "--watch"])).toThrow(/--stop/);
	});
	test("--android, --sim and --device choose the target", () => {
		expect(parseArgs(["--android"]).platform).toBe("android");
		expect(parseArgs(["--sim"]).platform).toBe("sim");
		expect(parseArgs(["--device", "My iPhone"]).device).toBe("My iPhone");
		expect(() => parseArgs(["--android", "--sim"])).toThrow(/one of/);
	});
	test("an unknown flag is an error, not silently ignored", () => {
		expect(() => parseArgs(["--wat"])).toThrow(/--wat/);
		expect(() => parseArgs(["--device"])).toThrow(/--device/);
	});
});

describe("pickDevice", () => {
	const devs: Dev[] = [
		{ id: "SIM-1", name: "iPhone 15 Pro", platform: "ios", emulator: true, supported: true },
		{ id: "MAC", name: "macOS", platform: "darwin", emulator: false, supported: true },
		{ id: "PHONE-1", name: "iPhone de Matheus", platform: "ios", emulator: false, supported: true },
		{ id: "PIXEL", name: "Pixel 7", platform: "android", emulator: false, supported: true },
		{ id: "EMU", name: "sdk gphone", platform: "android", emulator: true, supported: true },
	];
	test("the first connected PHYSICAL iPhone, never a simulator or the Mac", () => {
		expect(pickDevice(devs, parseArgs([])).id).toBe("PHONE-1");
	});
	test("--android picks a physical Android phone; --sim a booted simulator", () => {
		expect(pickDevice(devs, parseArgs(["--android"])).id).toBe("PIXEL");
		expect(pickDevice(devs, parseArgs(["--sim"])).id).toBe("SIM-1");
	});
	test("--device matches by id or by name, case-insensitively", () => {
		expect(pickDevice(devs, parseArgs(["--device", "pixel 7"])).id).toBe("PIXEL");
		expect(pickDevice(devs, parseArgs(["--device", "PHONE-1"])).id).toBe("PHONE-1");
	});
	// Saying "no iPhone is connected" beats building for nothing.
	test("no matching device is a clear error naming what to do", () => {
		expect(() => pickDevice(devs.filter((d) => d.platform !== "ios" || d.emulator), parseArgs([]))).toThrow(
			/no connected iPhone.*--sim/s,
		);
		expect(() => pickDevice([], parseArgs(["--android"]))).toThrow(/no connected Android/);
		expect(() => pickDevice(devs, parseArgs(["--device", "nope"]))).toThrow(/nope/);
	});
	test("an unsupported (offline, locked) device is not picked", () => {
		const off = devs.map((d) => (d.id === "PHONE-1" ? { ...d, supported: false } : d));
		expect(() => pickDevice(off, parseArgs([]))).toThrow(/no connected iPhone/);
	});
});

describe("build commands", () => {
	const sha = "a1b2c3d";
	test("iOS builds the lab scheme with the lab dart-defines and no push/onboarding noise", () => {
		const a = iosBuildArgs(sha);
		expect(a.slice(0, 3)).toEqual(["build", "ios", "--debug"]);
		expect(a).toContain("--flavor=lab");
		expect(a).toContain("--dart-define=ATLAS_LAB=1");
		expect(a).toContain(`--dart-define=ATLAS_SHA=${sha}`);
		expect(a).toContain("--dart-define=ATLAS_ALWAYS_ONBOARD=false");
		expect(a).not.toContain("--release");
	});
	// A simulator runs x86/arm64-simulator slices: a device build cannot be installed on it.
	test("a simulator target builds with --simulator, a phone does not", () => {
		expect(iosBuildArgs(sha, true)).toContain("--simulator");
		expect(iosBuildArgs(sha, false)).not.toContain("--simulator");
		expect(iosBuildArgs(sha)).not.toContain("--simulator");
	});
	test("Android passes the gradle property that makes the lab id, never a flavor", () => {
		const a = androidBuildArgs(sha);
		expect(a.slice(0, 3)).toEqual(["build", "apk", "--debug"]);
		expect(a).toContain("--android-project-arg=atlas.lab=1");
		expect(a.join(" ")).not.toContain("--flavor");
		expect(a).toContain("--dart-define=ATLAS_LAB=1");
	});
	// `flutter run --release` attached hangs (documented): the attached run is debug, hot reload, lab only.
	test("the attached --watch run is a debug `flutter run` of the lab, never --release", () => {
		const ios = runArgs("ios", "PHONE-1", sha);
		const and = runArgs("android", "PIXEL", sha);
		for (const a of [ios, and]) {
			expect(a[0]).toBe("run");
			expect(a).not.toContain("--release");
			expect(a).toContain("--debug");
			expect(a).toContain("--dart-define=ATLAS_LAB=1");
			expect(a.slice(a.indexOf("-d"), a.indexOf("-d") + 2)).toEqual(["-d", a[a.indexOf("-d") + 1]]);
		}
		expect(ios).toContain("--flavor=lab");
		expect(and).toContain("--android-project-arg=atlas.lab=1");
	});
});

describe("pairUrl", () => {
	const p = { node_id: "n1", code: "123456", hostname: "My Mac", addrs: ["192.168.0.5:4433", "127.0.0.1:4433"], relay_url: null };
	// Same URL the desktop shows (apps/desktop/src/lib/settings/mobile.ts), parsed by the phone's PairInfo.parse.
	test("is atlas-mobile://pair with node, code, name and every address", () => {
		const u = new URL(pairUrl(p));
		expect(u.protocol).toBe("atlas-mobile:");
		expect(u.searchParams.get("node")).toBe("n1");
		expect(u.searchParams.get("code")).toBe("123456");
		expect(u.searchParams.get("name")).toBe("My Mac");
		expect(u.searchParams.getAll("addr")).toEqual(p.addrs);
	});
	test("the relay is added when there is one", () => {
		expect(new URL(pairUrl({ ...p, relay_url: "https://r.example" })).searchParams.get("relay")).toBe("https://r.example");
	});
	// An emulator reaches this computer's loopback through an alias.
	test("an Android emulator also gets the loopback alias 10.0.2.2", () => {
		const addrs = new URL(pairUrl(p, "10.0.2.2")).searchParams.getAll("addr");
		expect(addrs).toContain("10.0.2.2:4433");
		expect(addrs).toContain("127.0.0.1:4433");
	});
});

describe("shortSha", () => {
	test("is git's short form, and `nogit` outside a repository", () => {
		expect(shortSha("a1b2c3d4e5f60718293a4b5c6d7e8f9012345678\n")).toBe("a1b2c3d");
		expect(shortSha("")).toBe("nogit");
	});
});

// The whole point: never act on the real installation. These are source guards on the script itself.
describe("the script never touches the main installation", () => {
	const src = readFileSync(join(import.meta.dir, "mobile-lab.ts"), "utf8");
	const code = src
		.split("\n")
		.filter((l) => !l.trim().startsWith("//") && !l.trim().startsWith("*"))
		.join("\n");
	test("it never stops, restarts or replaces the main hostd or the installed CLI", () => {
		expect(code).not.toMatch(/hostd["',\s]+--(restart|reconcile|upgrade)/);
		expect(code).not.toMatch(/\.atlas\/bin/);
		expect(code).not.toMatch(/atlas link|atlas unlink|desktop --sync/);
		// `--stop` is only ever sent to the daemon through `labHostd`, which always passes `--lab`.
		expect(code).toMatch(/\$`\$\{atlas\} hostd --lab \$\{args\}`/);
		expect(code).not.toMatch(/hostd(?! --lab)[^\n]*--stop/);
	});
	test("it never uses `flutter run --release` (it hangs attached)", () => {
		expect(code).not.toMatch(/"run"[^;]*"--release"/s);
	});
	test("the CLI is the repo's debug build, never the installed one", () => {
		expect(code).toMatch(/target\/debug\/atlas/);
	});
});
