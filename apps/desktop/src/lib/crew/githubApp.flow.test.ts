// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { beforeEach, describe, expect, mock, test } from "bun:test";
import fixture from "../__fixtures__/crew-github-app.json" with { type: "json" };

// The creation flow of a crew server's GitHub App against the real `CrewGithubApp` and the real `hostCall` (so a coded
// failure is rendered by the real catalog). Only the Tauri boundary is scripted: hostd answers with the payloads a real
// hostd wrote (`crew-github-app.json`), and the loopback listener commands are driven by the test.
type Call = { cmd: string; args: Record<string, unknown> };
let calls: Call[];
let hostd: Record<string, (args: Record<string, unknown>) => unknown>;
let codeArrives: (o: unknown) => void;
let waitFor: Promise<unknown>;
const FORM_URL = "http://127.0.0.1:5555/start/tok";

mock.module("@tauri-apps/api/core", () => ({
	invoke: async (cmd: string, args: Record<string, unknown>) => {
		if (cmd === "host_call") {
			calls.push({ cmd: args.cmd, args: args.args });
			return hostd[args.cmd](args.args);
		}
		calls.push({ cmd, args });
		if (cmd === "github_app_listen_start") return { id: "L1", url: FORM_URL, expires_in_secs: 600 };
		if (cmd === "github_app_listen_wait") return waitFor;
		if (cmd === "open_url" && failOpen) throw "no browser";
		return null;
	},
}));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
const uiKit = await import("../../../test/uiKit.ts");
await uiKit.installUiKit();
const { CrewGithubApp } = await import("./githubApp.svelte.ts");
const { m } = await import("#lib/i18n/m.ts");

let failOpen = false;
let now = 1_000_000;
let tick: () => void;
let stopped = 0;
const deps = () => ({
	now: () => now,
	every: (fn: () => void) => {
		tick = fn;
		return () => stopped++;
	},
});
const coded = (e: { code: string; args: object }, message = "English wire text") => ({ ...e, message });

const names = () => calls.map((c) => c.cmd);
const settle = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
	calls = [];
	failOpen = false;
	stopped = 0;
	now = 1_000_000;
	waitFor = new Promise((r) => (codeArrives = r));
	hostd = {
		crew_github_app_manifest: (a) =>
			a.owner_kind === "org" ? fixture.manifest_org_with_workflows : fixture.manifest_user,
		crew_github_app_convert: () => fixture.status,
		crew_github_app_status: () => fixture.status_without_app,
		crew_github_app_remove: () => ({}),
	};
});

async function waiting(app) {
	const run = app.create();
	await settle();
	expect(app.phase).toBe("waiting");
	// Wrapped: an async function returning the promise itself would make `await waiting()` wait for the whole flow,
	// whose code only arrives after the test calls `codeArrives`.
	return { run };
}

describe("create", () => {
	test("manifest, listener, browser, code, conversion, then GitHub's install page, in that order", async () => {
		const app = new CrewGithubApp("ssh:crew@box", deps());
		await app.load();
		expect(app.phase).toBe("none");
		const { run } = await waiting(app);
		codeArrives({ status: "code", code: "c0de" });
		await run;

		expect(calls.map((c) => c.cmd)).toEqual([
			"crew_github_app_status",
			"crew_github_app_manifest",
			"github_app_listen_start",
			"open_url",
			"github_app_listen_wait",
			"crew_github_app_convert",
			"open_url",
		]);
		expect(calls[1].args).toEqual({ host: "ssh:crew@box", owner_kind: "user", workflows: false });
		// The listener gets hostd's manifest and POST address untouched (it adds the redirect and the state).
		expect(calls[2].args).toEqual({
			manifest: fixture.manifest_user.manifest,
			postUrl: fixture.manifest_user.post_url,
		});
		expect(calls[3].args).toEqual({ url: FORM_URL });
		expect(calls[5].args).toEqual({ host: "ssh:crew@box", code: "c0de" });
		expect(calls[6].args).toEqual({ url: "https://github.com/apps/atlas-crew-box/installations/new" });
		expect(app.phase).toBe("ready");
		expect(app.app.slug).toBe("atlas-crew-box");
		expect(app.watching).toBe(true);
		expect(app.error).toBe("");
	});

	test("an organization with Workflows asks hostd for both, and the deadline is the listener's window", async () => {
		const app = new CrewGithubApp("h", deps());
		app.phase = "none";
		app.owner = "org";
		app.org = "acme";
		app.workflows = true;
		const { run } = await waiting(app);
		expect(calls[0].args).toEqual({ host: "h", owner_kind: "org", org: "acme", workflows: true });
		expect(calls[1].args.postUrl).toBe("https://github.com/organizations/acme/settings/apps/new");
		expect(app.secondsLeft(now + 100_000)).toBe(500);
		expect(app.secondsLeft(now + 700_000)).toBe(0);
		app.cancel();
		codeArrives({ status: "cancelled" });
		await run;
	});

	test("an organization without a valid login starts nothing", async () => {
		const app = new CrewGithubApp("h", deps());
		app.phase = "none";
		app.owner = "org";
		app.org = "not a login";
		await app.create();
		expect(calls).toEqual([]);
		expect(app.phase).toBe("none");
	});

	test("a second click while the flow runs does not open a second listener", async () => {
		const app = new CrewGithubApp("h", deps());
		app.phase = "none";
		const { run } = await waiting(app);
		await app.create();
		expect(names().filter((n) => n === "github_app_listen_start")).toHaveLength(1);
		codeArrives({ status: "cancelled" });
		await run;
	});

	test("a code spent after the card left the page is not converted", async () => {
		const app = new CrewGithubApp("h", deps());
		app.phase = "none";
		const { run } = await waiting(app);
		app.destroy();
		codeArrives({ status: "code", code: "late" });
		await run;
		expect(names()).not.toContain("crew_github_app_convert");
		expect(calls.find((c) => c.cmd === "github_app_listen_cancel")?.args).toEqual({ id: "L1" });
	});

	test("Cancel closes the listener and returns to the form", async () => {
		const app = new CrewGithubApp("h", deps());
		app.phase = "none";
		const { run } = await waiting(app);
		app.cancel();
		expect(calls.find((c) => c.cmd === "github_app_listen_cancel")?.args).toEqual({ id: "L1" });
		expect(app.phase).toBe("none");
		codeArrives({ status: "cancelled" });
		await run;
		expect(app.phase).toBe("none");
		expect(app.error).toBe("");
	});

	test("the listener giving up says nothing was created", async () => {
		const app = new CrewGithubApp("h", deps());
		app.phase = "none";
		const { run } = await waiting(app);
		codeArrives({ status: "timed_out" });
		await run;
		expect(app.phase).toBe("none");
		expect(app.error).toBe(m.crew_github_app_timed_out({ minutes: 10 }));
		expect(names()).not.toContain("crew_github_app_convert");
	});

	test("a browser that cannot be opened closes the listener and says why", async () => {
		failOpen = true;
		const app = new CrewGithubApp("h", deps());
		app.phase = "none";
		await app.create();
		expect(app.phase).toBe("none");
		expect(app.error).toBe("no browser");
		expect(names()).toContain("github_app_listen_cancel");
		expect(names()).not.toContain("github_app_listen_wait");
	});
});

describe("coded failures read through the hostd catalog", () => {
	test("an expired code shows the catalog sentence, not hostd's English, and the form is back", async () => {
		hostd.crew_github_app_convert = () => {
			throw coded(fixture.errors.code_expired);
		};
		const app = new CrewGithubApp("h", deps());
		app.phase = "none";
		const { run } = await waiting(app);
		codeArrives({ status: "code", code: "old" });
		await run;
		expect(app.phase).toBe("none");
		expect(app.error).toBe(m.hostd_wire_github_app_code_expired());
		expect(app.error).not.toBe("English wire text");
		expect(app.watching).toBe(false);
	});

	test("'exists' (another desktop got there first) shows the app that is there", async () => {
		hostd.crew_github_app_convert = () => {
			throw coded(fixture.errors.exists);
		};
		const app = new CrewGithubApp("h", deps());
		app.phase = "none";
		const { run } = await waiting(app);
		hostd.crew_github_app_status = () => fixture.status;
		codeArrives({ status: "code", code: "c" });
		await run;
		await settle();
		expect(app.error).toBe(m.hostd_wire_github_app_exists());
		expect(app.phase).toBe("ready");
		expect(app.app.slug).toBe("atlas-crew-box");
	});

	test("GitHub refusing the conversion shows its status", async () => {
		hostd.crew_github_app_convert = () => {
			throw coded({ code: "crew.github_app.conversion_failed", args: { status: 422 } });
		};
		const app = new CrewGithubApp("h", deps());
		app.phase = "none";
		const { run } = await waiting(app);
		codeArrives({ status: "code", code: "c" });
		await run;
		expect(app.error).toBe(m.hostd_wire_github_app_conversion_failed({ status: "422" }));
	});
});

describe("watching the install page", () => {
	async function ready() {
		hostd.crew_github_app_status = () => ({
			app: { ...fixture.status.app, installations: [] },
		});
		const app = new CrewGithubApp("h", deps());
		await app.load();
		expect(app.phase).toBe("ready");
		await app.openInstall();
		return app;
	}

	test("the installation appears on its own and the watch stops", async () => {
		const app = await ready();
		expect(app.watching).toBe(true);
		expect(app.app.installations).toEqual([]);
		hostd.crew_github_app_status = () => fixture.status;
		tick();
		await settle();
		expect(app.app.installations).toHaveLength(1);
		expect(app.watching).toBe(false);
		expect(stopped).toBe(1);
	});

	test("nothing changes: it keeps asking, a failed ask changes nothing shown, and it gives up after ten minutes", async () => {
		const app = await ready();
		tick();
		await settle();
		hostd.crew_github_app_status = () => {
			throw "Terminal host not reachable.";
		};
		tick();
		await settle();
		expect(app.watching).toBe(true);
		expect(app.app.installations).toEqual([]);
		expect(app.error).toBe("");
		now += 10 * 60 * 1000;
		tick();
		await settle();
		expect(app.watching).toBe(false);
	});

	test("ticks do not overlap while an ask is still out", async () => {
		const app = await ready();
		let release;
		hostd.crew_github_app_status = () => new Promise((r) => (release = () => r(fixture.status)));
		const before = names().filter((n) => n === "crew_github_app_status").length;
		tick();
		tick();
		tick();
		await settle();
		expect(names().filter((n) => n === "crew_github_app_status").length - before).toBe(1);
		release();
		await settle();
	});

	test("the app deleted elsewhere ends the watch and shows the form", async () => {
		const app = await ready();
		hostd.crew_github_app_status = () => fixture.status_without_app;
		tick();
		await settle();
		expect(app.phase).toBe("none");
		expect(app.watching).toBe(false);
	});
});

describe("refresh and remove", () => {
	test("Refresh asks again and shows what changed", async () => {
		hostd.crew_github_app_status = () => fixture.status;
		const app = new CrewGithubApp("h", deps());
		await app.load();
		hostd.crew_github_app_status = () => ({
			app: { ...fixture.status.app, installations: [], installations_error: "GitHub answered 502" },
		});
		await app.refresh();
		expect(app.app.installationsError).toBe("GitHub answered 502");
	});

	test("Remove deletes on the server and shows the form; an app that is already gone is not an error", async () => {
		hostd.crew_github_app_status = () => fixture.status;
		const app = new CrewGithubApp("h", deps());
		await app.load();
		hostd.crew_github_app_remove = () => {
			throw coded({ code: "crew.github_app.none", args: {} });
		};
		hostd.crew_github_app_status = () => fixture.status_without_app;
		await app.remove();
		expect(app.app).toBeNull();
		expect(app.phase).toBe("none");
		expect(app.error).toBe("");
	});

	test("a status call that fails says so and leaves the form usable", async () => {
		hostd.crew_github_app_status = () => {
			throw "boom";
		};
		const app = new CrewGithubApp("h", deps());
		await app.load();
		expect(app.phase).toBe("none");
		expect(app.error).toBe(m.crew_github_app_load_failed({ detail: "boom" }));
	});
});
