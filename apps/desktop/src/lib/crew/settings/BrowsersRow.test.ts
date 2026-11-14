// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../../test/setup.ts";
import { afterEach, beforeEach, describe, expect, mock, test } from "bun:test";
import fixture from "../../../__fixtures__/crew_browsers_status.json" with { type: "json" };

// A crew server's settings row for the capture browsers, against the real `BrowsersRow` and `CrewBrowsers`. Only the
// Tauri boundary is stubbed: `crew_browsers_status` answers what the Rust command serialized for the REAL output of
// `atlas crew browsers status` (`crew_browsers_status.json`), and the install streams lines on `crew://install-line`.
const calls: { cmd: string; args: Record<string, unknown> }[] = [];
let status: unknown = fixture.missing;
let installError: string | null = null;
let lineHandler: ((e: { payload: { line: string; is_err: boolean } }) => void) | undefined;
mock.module("@tauri-apps/api/core", () => ({
	invoke: async (cmd: string, args: Record<string, unknown>) => {
		calls.push({ cmd, args });
		if (cmd === "crew_browsers_status") {
			if (status instanceof Error) throw status.message;
			return status;
		}
		if (cmd === "crew_browsers_install") {
			for (const line of ["step: browsers", "downloading chromium", "downloading webkit"]) {
				lineHandler?.({ payload: { line, is_err: false } });
			}
			if (installError) throw installError;
			status = fixture.ready;
			return null;
		}
		return {};
	},
}));
mock.module("@tauri-apps/api/event", () => ({
	listen: async (name: string, cb: typeof lineHandler) => {
		if (name === "crew://install-line") lineHandler = cb;
		return () => {};
	},
}));
const uiKit = await import("../../../../test/uiKit.ts");
await uiKit.installUiKit();
await uiKit.useRealKit("Button", "Spinner");

// Dynamic imports on purpose: the Svelte loader registered by test/setup.ts and the module mocks above must exist
// before these modules load.
const { flushSync, mount, unmount, tick } = await import("svelte");
const { default: BrowsersRow } = await import("./BrowsersRow.svelte");

let done: (() => void) | undefined;
beforeEach(() => {
	calls.length = 0;
	status = fixture.missing;
	installError = null;
	lineHandler = undefined;
});
afterEach(() => {
	done?.();
	done = undefined;
});

const settle = async () => {
	for (let i = 0; i < 20; i++) {
		for (let j = 0; j < 10; j++) await Promise.resolve();
		flushSync();
		await tick();
	}
};

function render() {
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() => mount(BrowsersRow, { target, props: { hostKey: "ssh:atlas-crew@srv" } }));
	done = () => (unmount(app), target.remove());
	return target;
}

const button = (t, label) => [...t.querySelectorAll("button")].find((b) => b.textContent.trim() === label);
async function type(t, label, value) {
	const input = t.querySelector(`input[aria-label="${label}"]`);
	input.value = value;
	input.dispatchEvent(new Event("input", { bubbles: true }));
	await settle();
}
async function check(t, address = "ubuntu@srv") {
	await type(t, "Admin SSH address", address);
	button(t, "Check").click();
	await settle();
}

describe("the capture browsers row on a crew server's settings", () => {
	test("says why and what it costs, and asks nothing of the server until Check", async () => {
		const t = render();
		await settle();
		expect(t.textContent).toContain("Screenshots for UI work");
		expect(t.textContent).toContain("capture, UI gate and locate");
		expect(t.textContent).toContain("Node.js 20+");
		expect(t.textContent).toContain("500 MB");
		expect(calls).toEqual([]);
		expect(button(t, "Check").disabled).toBe(true);
	});

	test("Node 18 on the server: the requirement is stated plainly and Install is not offered", async () => {
		const t = render();
		await check(t);
		expect(calls[0]).toEqual({ cmd: "crew_browsers_status", args: { target: "ubuntu@srv", port: null } });
		const alert = t.querySelector(".node");
		expect(alert.textContent).toContain("Node.js 20 or newer is required.");
		expect(alert.textContent).toContain("Node 18.19.1 found, need >= 20");
		expect(button(t, "Install").disabled).toBe(true);
		// The other missing pieces are listed without the CLI's "run atlas crew browsers install" tail.
		const items = [...t.querySelectorAll(".problems li")].map((li) => li.textContent);
		expect(items).toEqual([
			"runner: missing",
			"playwright: package missing",
			"browsers: chromium missing",
			"browsers: webkit missing",
		]);
	});

	test("a port in the address goes to the command as the port", async () => {
		const t = render();
		await check(t, "ubuntu@srv:2222");
		expect(calls[0].args).toEqual({ target: "ubuntu@srv", port: 2222 });
	});

	test("a ready server says installed and has no Install button", async () => {
		status = fixture.ready;
		const t = render();
		await check(t);
		expect(t.querySelector('[role="status"]').textContent).toContain("Chromium and WebKit are ready");
		expect(button(t, "Install")).toBeUndefined();
	});

	test("Install runs the command, streams its lines into the log and checks again", async () => {
		status = { ready: false, problems: ["browsers: webkit missing, run atlas crew browsers install"] };
		const t = render();
		await check(t);
		const install = button(t, "Install");
		expect(install.disabled).toBe(false);
		install.click();
		await settle();
		expect(calls.map((c) => c.cmd)).toEqual([
			"crew_browsers_status",
			"crew_browsers_install",
			"crew_browsers_status",
		]);
		expect(calls[1].args).toEqual({ target: "ubuntu@srv", port: null });
		// The `step:` marker is progress plumbing, not log text.
		expect(t.querySelector(".log").textContent).toBe("downloading chromium\ndownloading webkit");
		expect(t.querySelector('[role="status"]').textContent).toContain("ready");
	});

	test("an install that fails shows the CLI's error and keeps Install available to retry", async () => {
		status = { ready: false, problems: ["runner: missing, run atlas crew browsers install"] };
		installError = "npm ci on ubuntu@srv failed: ENOSPC";
		const t = render();
		await check(t);
		button(t, "Install").click();
		await settle();
		expect(t.querySelector(".error").textContent).toBe("npm ci on ubuntu@srv failed: ENOSPC");
		expect(button(t, "Install").disabled).toBe(false);
		expect(calls.filter((c) => c.cmd === "crew_browsers_status")).toHaveLength(1);
	});

	test("a sudo that asks for a password shows the field, and the retry sends it for that call", async () => {
		status = { ready: false, problems: ["runner: missing, run atlas crew browsers install"] };
		installError = "sudo on ubuntu@srv asks for a password — grant passwordless sudo to this user";
		const t = render();
		await check(t);
		button(t, "Install").click();
		await settle();
		expect(t.querySelector('input[aria-label="Sudo password"]')).not.toBeNull();
		installError = null;
		await type(t, "Sudo password", "s3cret");
		button(t, "Install").click();
		await settle();
		const retry = calls.filter((c) => c.cmd === "crew_browsers_install")[1];
		expect(retry.args).toEqual({ target: "ubuntu@srv", port: null, sudoPassword: "s3cret" });
		// Done: the password is gone from the page.
		expect(t.querySelector('input[aria-label="Sudo password"]')).toBeNull();
	});

	test("an unreachable server is an error, never an empty list of missing pieces", async () => {
		status = new Error("cannot reach ubuntu@srv");
		const t = render();
		await check(t);
		expect(t.querySelector(".error").textContent).toBe("cannot reach ubuntu@srv");
		expect(t.querySelector(".problems")).toBeNull();
	});
});
