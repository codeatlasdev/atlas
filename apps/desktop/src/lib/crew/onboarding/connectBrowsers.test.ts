// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../../test/setup.ts";
import { afterEach, beforeEach, describe, expect, mock, test } from "bun:test";

// The crew-server install offers the capture browsers as an opt-in (off by default). Against the real `Onboarding`
// and the real `ConnectStep`/`InstallChecklist`; only the Tauri boundary is stubbed, so what `crew_server_install`
// receives is what the Rust command parses (`browsers`, next to `target`, `port`, `sudoPassword`).
// The stores `Onboarding` pulls in call Tauri at import time; there is no Tauri in a test.
window.__TAURI_INTERNALS__ = { invoke: async () => null, transformCallback: () => 0 };
// `agents.svelte.ts` loads the real `@tauri-apps/api/window`, whose `dpi.js` links `SERIALIZE_TO_IPC_FN` from
// `api/core`: this file's `core` mock (`{ invoke }` only, module mocks are process-wide) would make that link fail.
// The onboarding never touches the window, so the window module is replaced.
mock.module("@tauri-apps/api/window", () => ({
	getCurrentWindow: () => ({ onFocusChanged: async () => () => {} }),
}));
const calls: { cmd: string; args: Record<string, unknown> }[] = [];
let lineHandler: ((e: { payload: { line: string; is_err: boolean } }) => void) | undefined;
mock.module("@tauri-apps/api/core", () => ({
	invoke: async (cmd: string, args: Record<string, unknown>) => {
		calls.push({ cmd, args });
		if (cmd === "crew_servers_list") return [];
		if (cmd === "ssh_key_find") return "/Users/me/.ssh/id_ed25519.pub";
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
await uiKit.useRealKit("Checkbox", "CopyButton", "Button");

// Dynamic imports on purpose: the Svelte loader registered by test/setup.ts and the module mocks above must exist
// before these modules load.
const { flushSync, mount, unmount, tick } = await import("svelte");
const { default: ConnectStep } = await import("./ConnectStep.svelte");
const { default: InstallChecklist } = await import("./InstallChecklist.svelte");
const { Onboarding } = await import("./onboarding.svelte.ts");

let done: (() => void) | undefined;
beforeEach(() => {
	calls.length = 0;
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

function render(Component, props) {
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() => mount(Component, { target, props }));
	done = () => (unmount(app), target.remove());
	return target;
}

async function connectStep() {
	const ob = new Onboarding(() => {});
	ob.step = "connect";
	ob.hasKey = true;
	ob.keyChecked = true;
	ob.sshTarget = "ubuntu@srv";
	const t = render(ConnectStep, { ob });
	await settle();
	return { ob, t };
}

const installCall = () => calls.find((c) => c.cmd === "crew_server_install");

describe("the browsers opt-in on the connect step", () => {
	test("is offered, says why and what it costs, and is off by default", async () => {
		const { ob, t } = await connectStep();
		const box = t.querySelector('input[type="checkbox"]');
		expect(box).not.toBeNull();
		expect(box.checked).toBe(false);
		expect(ob.browsers).toBe(false);
		const text = t.textContent;
		expect(text).toContain("Screenshots for UI work (Chromium + WebKit)");
		expect(text).toContain("capture, UI gate and locate");
		expect(text).toContain("Node.js 20+");
		expect(text).toContain("500 MB");
	});

	test("a first install without the opt-in does not ask for browsers", async () => {
		const { ob } = await connectStep();
		await ob.launchServer();
		expect(installCall().args.browsers).toBe(false);
	});

	test("ticking it passes the flag, with the port and the sudo password the install already took", async () => {
		const { ob, t } = await connectStep();
		t.querySelector('input[type="checkbox"]').click();
		await settle();
		expect(ob.browsers).toBe(true);
		ob.sshPort = 2222;
		await ob.launchServer("s3cret");
		expect(installCall().args).toEqual({
			target: "ubuntu@srv",
			port: 2222,
			browsers: true,
			sudoPassword: "s3cret",
		});
	});

	test("the terminal command the disclosure offers matches the opt-in", async () => {
		const { ob, t } = await connectStep();
		expect(t.querySelector(".code-block code").textContent).toBe("atlas crew install ubuntu@srv");
		t.querySelector('input[type="checkbox"]').click();
		await settle();
		expect(t.querySelector(".code-block code").textContent).toBe("atlas crew install ubuntu@srv --browsers");
	});

	test("the checkbox is locked while the install runs", async () => {
		const { ob, t } = await connectStep();
		ob.installRunning = true;
		await settle();
		expect(t.querySelector('input[type="checkbox"]').disabled).toBe(true);
	});
});

describe("the install checklist", () => {
	const labels = (t) => [...t.querySelectorAll(".checklist-label")].map((e) => e.textContent);

	test("has a browsers step only when the install includes it", async () => {
		const base = render(InstallChecklist, { done: new Set(), active: null, failed: null });
		expect(labels(base)).not.toContain("Installing screenshot browsers");
		done();
		const withBrowsers = render(InstallChecklist, {
			done: new Set(),
			active: "browsers",
			failed: null,
			browsers: true,
		});
		const l = labels(withBrowsers);
		expect(l.indexOf("Installing screenshot browsers")).toBe(l.indexOf("Connecting") - 1);
		expect(withBrowsers.querySelector(".checklist-row.active .checklist-label").textContent).toBe(
			"Installing screenshot browsers",
		);
	});
});
