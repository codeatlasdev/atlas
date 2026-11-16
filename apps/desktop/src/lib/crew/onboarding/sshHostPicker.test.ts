// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../../test/setup.ts";
import { afterEach, beforeEach, describe, expect, mock, test } from "bun:test";
import hostsFixture from "../../__fixtures__/ssh-hosts.json";

// "Add a crew server" offers the servers already in ~/.ssh/config (#502). Against the real `Onboarding`, the real
// `ConnectStep`, the real picker and the kit's real Combobox; only the Tauri boundary is stubbed, and what
// `ssh_hosts_list` returns is the REAL serialization of the Rust `SshHost` (`__fixtures__/ssh-hosts.json`, written by
// `UPDATE_FIXTURES=1 cargo test -p atlas-core ssh_config`), never a hand-written payload.
window.__TAURI_INTERNALS__ = { invoke: async () => null, transformCallback: () => 0 };
mock.module("@tauri-apps/api/window", () => ({
	getCurrentWindow: () => ({ onFocusChanged: async () => () => {} }),
}));
const calls: { cmd: string; args: Record<string, unknown> }[] = [];
let hostsAnswer: () => unknown = () => hostsFixture;
mock.module("@tauri-apps/api/core", () => ({
	invoke: async (cmd: string, args: Record<string, unknown>) => {
		calls.push({ cmd, args });
		if (cmd === "crew_servers_list") return [];
		if (cmd === "ssh_key_find") return "/Users/me/.ssh/id_ed25519.pub";
		if (cmd === "ssh_hosts_list") return hostsAnswer();
		return {};
	},
}));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
const uiKit = await import("../../../../test/uiKit.ts");
await uiKit.installUiKit();
await uiKit.useRealKit("Checkbox", "CopyButton", "Button", "Combobox");

const { flushSync, mount, unmount, tick } = await import("svelte");
// Dynamic imports on purpose: the Svelte loader registered by test/setup.ts and the module mocks above must exist
// before these modules load.
const { default: ConnectStep } = await import("./ConnectStep.svelte");
const { Onboarding } = await import("./onboarding.svelte.ts");

let done: (() => void) | undefined;
beforeEach(() => {
	calls.length = 0;
	hostsAnswer = () => hostsFixture;
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

/** The connect step as the onboarding shows it: `checkSshKey` is what loads the key and the hosts. */
async function connectStep() {
	const ob = new Onboarding(() => {});
	ob.step = "connect";
	ob.checkSshKey();
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() => mount(ConnectStep, { target, props: { ob } }));
	done = () => (unmount(app), target.remove());
	await settle();
	return { ob, t: target };
}

const trigger = (t) => t.querySelector("button.ca-combobox__trigger");
const searchInput = () => document.querySelector("input.ca-combobox__input");
const options = () => [...document.querySelectorAll('[role="option"]')];
const addressField = (t) => t.querySelector("#ssh-target");
const portField = (t) => t.querySelector("#ssh-port");

async function openPicker(t) {
	trigger(t).click();
	await settle();
}

async function typeInSearch(text) {
	const input = searchInput();
	input.value = text;
	input.dispatchEvent(new Event("input", { bubbles: true }));
	await settle();
}

async function press(el, key) {
	el.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
	await settle();
}

describe("the picker of ~/.ssh/config servers on the connect step", () => {
	test("lists every server with where it goes and loads them once, when the step shows", async () => {
		const { t } = await connectStep();
		expect(calls.filter((c) => c.cmd === "ssh_hosts_list")).toHaveLength(1);
		await openPicker(t);
		expect(options().map((o) => o.textContent.trim())).toEqual([
			"prod · deploy@203.0.113.7:2222",
			"nohost · alice@nohost",
		]);
	});

	test("picking a server fills the address and the port, and the install goes to exactly that", async () => {
		const { ob, t } = await connectStep();
		await openPicker(t);
		options()[0].click();
		await settle();

		expect(addressField(t).value).toBe("deploy@prod");
		expect(portField(t).value).toBe("2222");
		expect(t.textContent).toContain("ssh prod");
		expect(t.textContent).toContain("~/.ssh/id_work");

		// Port 22 is sent as no port, like a typed address.
		await ob.launchServer();
		const install = calls.find((c) => c.cmd === "crew_server_install");
		expect(install.args.target).toBe("deploy@prod");
		expect(install.args.port).toBe(2222);
	});

	test("typing filters on the alias, the user, the host name and the port", async () => {
		const { t } = await connectStep();
		await openPicker(t);
		for (const [q, want] of [
			["prod", ["prod"]],
			["203.0.113", ["prod"]],
			["alice", ["nohost"]],
			["2222", ["prod"]],
			["PROD", ["prod"]],
		]) {
			await typeInSearch(q);
			expect(
				options().map((o) => o.textContent.split("·")[0].trim()),
				q,
			).toEqual(want);
		}
		await typeInSearch("zzz");
		expect(options()).toHaveLength(0);
		expect(document.body.textContent).toContain("No server matches");
	});

	test("is keyboard accessible: arrows move, Enter picks, and the typed letters stay in the field", async () => {
		const { ob, t } = await connectStep();
		await openPicker(t);
		const input = searchInput();
		// A letter that starts an alias must type, not jump the highlight (the kit's type-to-select).
		const letter = new KeyboardEvent("keydown", { key: "n", bubbles: true, cancelable: true });
		input.dispatchEvent(letter);
		expect(letter.defaultPrevented).toBe(false);

		await press(input, "ArrowDown");
		await press(input, "Enter");
		expect(ob.sshTarget).toBe("alice@nohost");
		expect(ob.sshPort).toBe(22);
		expect(addressField(t).value).toBe("alice@nohost");
	});

	test("editing the address or the port by hand drops the choice instead of keeping a stale label", async () => {
		const { ob, t } = await connectStep();
		await openPicker(t);
		options()[0].click();
		await settle();
		expect(trigger(t).textContent).toContain("prod");

		ob.sshTarget = "deploy@elsewhere";
		await settle();
		expect(trigger(t).textContent).not.toContain("prod ·");
		expect(t.textContent).not.toContain("Connects the way");

		ob.sshTarget = "deploy@prod";
		ob.sshPort = 22;
		await settle();
		expect(trigger(t).textContent).not.toContain("prod ·");
	});

	test("is disabled while an install runs", async () => {
		const { ob, t } = await connectStep();
		ob.installRunning = true;
		await settle();
		expect(trigger(t).disabled).toBe(true);
	});
});

describe("without a usable ~/.ssh/config", () => {
	for (const [name, answer] of [
		["no hosts", () => []],
		[
			"the command failing",
			() => {
				throw "cannot read ~/.ssh/config";
			},
		],
	]) {
		test(`${name}: just the free-text address and port, with no picker and nothing alarming`, async () => {
			hostsAnswer = answer;
			const { ob, t } = await connectStep();
			expect(trigger(t)).toBeNull();
			expect(addressField(t)).not.toBeNull();
			expect(portField(t)).not.toBeNull();
			expect(t.textContent).not.toMatch(/ssh\/config|error|fail/i);
			expect(ob.sshHosts).toEqual([]);

			// Typing an address still works as before.
			ob.sshTarget = "ubuntu@my-server.com";
			await ob.launchServer();
			expect(calls.find((c) => c.cmd === "crew_server_install").args.target).toBe("ubuntu@my-server.com");
		});
	}
});
