// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../../test/setup.ts";
import { afterEach, beforeEach, describe, expect, mock, test } from "bun:test";
import keyFixture from "../../__fixtures__/crew-key-lookup.json";
import errorFixture from "../../__fixtures__/crew-connect-errors.json";

// "Add a crew server" signs in with an SSH key (which one is shown, another can be picked) or with the account
// password, chosen up front (#517). Against the real `Onboarding`, `ConnectStep` and `ConnectActions`; only the Tauri
// boundary is stubbed, and every payload Rust sends (the key lookup, the connect errors) is the REAL serialization
// from `__fixtures__` (`UPDATE_FIXTURES=1 cargo test -p atlas-desktop crew_ssh`).
window.__TAURI_INTERNALS__ = { invoke: async () => null, transformCallback: () => 0 };
mock.module("@tauri-apps/api/window", () => ({
	getCurrentWindow: () => ({ onFocusChanged: async () => () => {} }),
}));
let picked: string | null = null;

const calls: { cmd: string; args: Record<string, unknown> }[] = [];
/** What each command answers; a function may throw to reject like a failing Rust command. */
let answers: Record<string, (args: Record<string, unknown>) => unknown> = {};
mock.module("@tauri-apps/api/core", () => ({
	invoke: async (cmd: string, args: Record<string, unknown>) => {
		calls.push({ cmd, args });
		if (answers[cmd]) return answers[cmd](args);
		if (cmd === "crew_servers_list" || cmd === "ssh_hosts_list") return [];
		if (cmd === "ssh_key_resolve") return keyFixture.found;
		if (cmd === "pick_path") return picked;
		return {};
	},
}));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
const uiKit = await import("../../../../test/uiKit.ts");
await uiKit.installUiKit();
await uiKit.useRealKit("Checkbox", "CopyButton", "Button", "TextField", "ToggleGroup/SegmentedControl");

// Dynamic imports on purpose: the Svelte loader registered by test/setup.ts and the module mocks above must exist
// before these modules load.
const { flushSync, mount, unmount, tick } = await import("svelte");
const { default: ConnectStep } = await import("./ConnectStep.svelte");
const { Onboarding } = await import("./onboarding.svelte.ts");

let done: (() => void) | undefined;
beforeEach(() => {
	calls.length = 0;
	answers = {};
	picked = null;
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

/** The connect step with its buttons, after the key lookup answered. */
async function connectStep(address = "ubuntu@203.0.113.7") {
	const ob = new Onboarding(() => {});
	ob.step = "connect";
	ob.checkSshKey();
	ob.sshTarget = address;
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() =>
		mount(
			// A tiny wrapper is not needed: mount both side by side in one container.
			ConnectStep,
			{ target, props: { ob } },
		),
	);
	done = () => (unmount(app), target.remove());
	await settle();
	return { ob, t: target, a: ob };
}

// The footer lives in the shell; its primary action is `ob.primary` (label, disabled, busy).
async function terminalCmd(t) {
	[...t.querySelectorAll("button")].find((b) => b.textContent.includes("Prefer the terminal?")).click();
	await settle();
	return t.querySelector(".popover .code-block code").textContent;
}
const primary = (ob) => ({
	textContent: ob.primary.label,
	disabled: ob.primary.disabled,
	click: () => ob.primary.run(),
});
const passwordField = (t) => t.querySelector('[name="ssh-password"]');
const failureBlock = (t) => t.querySelector('[data-testid="connect-failure"]');
const rejectWith = (name) => () => {
	throw errorFixture.find((c) => c.name === name).error;
};
const typeInto = async (input, text) => {
	input.value = text;
	input.dispatchEvent(new Event("input", { bubbles: true }));
	await settle();
};
const called = (cmd) => calls.filter((c) => c.cmd === cmd);

describe("the form shows which key will be used", () => {
	test("the key ssh resolves is on screen, SSH key is the method, and no Mac is mentioned", async () => {
		const { ob, t } = await connectStep();
		expect(ob.method).toBe("key");
		expect(t.querySelector('[data-testid="key-in-use"]').textContent).toContain("/Users/me/.ssh/id_ed25519");
		expect(passwordField(t)).toBeNull();
		expect(t.textContent).not.toMatch(/\bMac(s|'s|Book)?\b/);
		// The lookup was asked about the address typed, so a ~/.ssh/config alias gets its own key.
		ob.sshTarget = "deploy@prod";
		await ob.resolveKey();
		expect(called("ssh_key_resolve").at(-1).args).toEqual({
			target: "deploy@prod",
			port: null,
			identity: null,
		});
	});

	test("with no key the form starts on the password, which creates the key", async () => {
		answers.ssh_key_resolve = () => keyFixture.none;
		const { ob, t, a } = await connectStep();
		expect(ob.method).toBe("password");
		expect(passwordField(t)).not.toBeNull();
		expect(t.textContent).toContain("never stored");
		// Nothing to click until the password is typed.
		expect(primary(a).textContent.trim()).toBe("Add crew server");
		expect(primary(a).disabled).toBe(true);
		await typeInto(passwordField(t), "s3cret");
		expect(primary(a).disabled).toBe(false);
	});

	test("the person's own choice of method is not undone by a later lookup", async () => {
		answers.ssh_key_resolve = () => keyFixture.none;
		const { ob, t } = await connectStep();
		ob.chooseMethod("key");
		await ob.resolveKey();
		await settle();
		expect(ob.method).toBe("key");
		expect(t.querySelector('[data-testid="no-key"]')).not.toBeNull();
	});

	test("without ssh (Windows before the OpenSSH Client is added) the form says how to get it and blocks the install", async () => {
		answers.ssh_key_resolve = () => keyFixture.no_ssh;
		const { t, a } = await connectStep();
		expect(failureBlock(t).dataset.kind).toBe("ssh_missing");
		expect(primary(a).disabled).toBe(true);
	});
});

describe("choosing another key", () => {
	test("the picked file is the key for the lookup, the install and the registry; a .pub means its private key", async () => {
		const { ob } = await connectStep();
		picked = "C:\\Users\\me\\.ssh\\id_work.pub";
		answers.ssh_key_resolve = (args) =>
			args.identity
				? { ssh_found: true, key: { identity: args.identity, has_public: true } }
				: keyFixture.found;
		await ob.pickKeyFile();
		expect(ob.identity).toBe("C:\\Users\\me\\.ssh\\id_work");
		expect(ob.key.identity).toBe("C:\\Users\\me\\.ssh\\id_work");

		await ob.launchServer();
		expect(called("ssh_connect_check")[0].args.identity).toBe("C:\\Users\\me\\.ssh\\id_work");
		expect(called("crew_server_install")[0].args.identity).toBe("C:\\Users\\me\\.ssh\\id_work");
		// Every later connection (the tunnel, the relay, Settings) must use the same key: it goes in the registry.
		expect(called("crew_servers_add")[0].args.admin.identity).toBe("C:\\Users\\me\\.ssh\\id_work");
	});

	test("cancelling the picker changes nothing, and 'Use the default' goes back to what ssh finds", async () => {
		const { ob } = await connectStep();
		picked = null;
		await ob.pickKeyFile();
		expect(ob.identity).toBe("");
		ob.identity = "/k/other";
		ob.useDefaultKey();
		await settle();
		expect(ob.identity).toBe("");
		expect(called("ssh_key_resolve").at(-1).args.identity).toBeNull();
	});

	test("the terminal command carries the picked key", async () => {
		const { ob, t } = await connectStep();
		ob.identity = "/k/id work";
		await settle();
		expect(await terminalCmd(t)).toBe('atlas crew install ubuntu@203.0.113.7 --identity "/k/id work"');
	});
});

describe("signing in with the password", () => {
	test("adds the key first, then installs with the key; the password goes only to the sign-in call", async () => {
		answers.ssh_key_resolve = () => keyFixture.none;
		// The key Atlas created for the sign-in: the install must use exactly that one (`-i`).
		answers.ssh_authorize_key = () => ({ identity: "/Users/me/.ssh/id_new", has_public: true });
		const { ob, t } = await connectStep();
		await typeInto(passwordField(t), "s3cret");
		await ob.launchServer();

		expect(
			calls
				.map((c) => c.cmd)
				.filter((c) => ["ssh_authorize_key", "ssh_connect_check", "crew_server_install"].includes(c)),
		).toEqual(["ssh_authorize_key", "crew_server_install"]);
		expect(called("crew_server_install")[0].args.identity).toBe("/Users/me/.ssh/id_new");
		expect(called("ssh_authorize_key")[0].args).toEqual({
			target: "ubuntu@203.0.113.7",
			port: null,
			identity: null,
			password: "s3cret",
		});
		expect(JSON.stringify(calls.filter((c) => c.cmd !== "ssh_authorize_key"))).not.toContain("s3cret");
		// Done once: the password is gone from the form and the method is the key from now on.
		expect(ob.sshPassword).toBe("");
		expect(ob.method).toBe("key");
		expect(ob.installDone).toBe(true);
	});

	test("a wrong password keeps the password method and says so, without a second 'Permission denied (publickey)' advice", async () => {
		answers.ssh_key_resolve = () => keyFixture.none;
		answers.ssh_authorize_key = rejectWith("password refused");
		const { ob, t } = await connectStep();
		await typeInto(passwordField(t), "wrong");
		await ob.launchServer();
		await settle();
		expect(ob.method).toBe("password");
		expect(ob.sshPassword).toBe("wrong");
		expect(failureBlock(t).dataset.kind).toBe("password_refused");
		expect(called("crew_server_install")).toHaveLength(0);
		expect(ob.failedPhase).toBe("signin");
	});

	test("a server that takes only keys offers the key method", async () => {
		answers.ssh_key_resolve = () => keyFixture.none;
		answers.ssh_authorize_key = rejectWith("server takes only keys");
		const { ob, t } = await connectStep();
		await typeInto(passwordField(t), "pw");
		await ob.launchServer();
		await settle();
		const use = [...failureBlock(t).querySelectorAll("button")].find((b) =>
			b.textContent.includes("Use my SSH key"),
		);
		use.click();
		await settle();
		expect(ob.method).toBe("key");
		expect(failureBlock(t)).toBeNull();
	});

	// The password was good and the key is on the server: a later failure of the install must not ask for the
	// password again, and "key refused" after that is not "authorize it with your password".
	test("after the key was added a retry goes with the key and does not ask for the password again", async () => {
		answers.ssh_key_resolve = () => keyFixture.none;
		answers.crew_server_install = rejectWith("host unreachable");
		const { ob, t } = await connectStep();
		await typeInto(passwordField(t), "s3cret");
		await ob.launchServer();
		expect(ob.method).toBe("key");
		expect(called("ssh_authorize_key")).toHaveLength(1);

		answers.crew_server_install = () => ({});
		await ob.launchServer();
		expect(called("ssh_authorize_key")).toHaveLength(1);
		expect(called("ssh_connect_check")).toHaveLength(1);
		expect(called("crew_server_install")).toHaveLength(2);
	});

	test("moving to another address forgets that the key was added", async () => {
		answers.ssh_key_resolve = () => keyFixture.none;
		answers.crew_server_install = rejectWith("key refused by the server");
		const { ob, t } = await connectStep();
		await typeInto(passwordField(t), "s3cret");
		await ob.launchServer();
		expect(ob.keyAdded).toBe(true);
		expect(ob.failure.title).toContain("still refuses");
		ob.sshTarget = "ubuntu@198.51.100.9";
		expect(ob.keyAdded).toBe(false);
	});
});

describe("when the install fails", () => {
	test("a refused key names the key and offers the password and another key, with the raw line under Details", async () => {
		answers.crew_server_install = rejectWith("key refused by the server");
		const { ob, t } = await connectStep();
		await ob.launchServer();
		await settle();
		const f = failureBlock(t);
		expect(f.dataset.kind).toBe("key_refused");
		expect(f.textContent).toContain("/Users/me/.ssh/id_ed25519");
		const buttons = [...f.querySelectorAll("button")].map((b) => b.textContent.trim());
		expect(buttons).toEqual(["Sign in with the password", "Choose another key", "How to fix"]);
		expect(f.querySelector("details code").textContent).toContain("Permission denied (publickey)");

		[...f.querySelectorAll("button")][0].click();
		await settle();
		expect(ob.method).toBe("password");
		expect(passwordField(t)).not.toBeNull();
		expect(failureBlock(t)).toBeNull();
	});

	test("the failed step is the one that was running", async () => {
		answers.ssh_connect_check = rejectWith("host unreachable");
		const { ob } = await connectStep();
		await ob.launchServer();
		expect(ob.failedPhase).toBe("signin");
		expect(called("crew_server_install")).toHaveLength(0);
	});

	test("a masked unit shows the CLI's hint and not a bare error line", async () => {
		answers.crew_server_install = rejectWith("masked unit with the CLI's hint");
		const { ob, t } = await connectStep();
		await ob.launchServer();
		await settle();
		expect(failureBlock(t).textContent).toContain("unmask atlas-crew.service");
		expect(failureBlock(t).querySelector("details code").textContent).toContain("is masked");
	});

	test("sudo that wants a password asks for it and the retry sends it", async () => {
		answers.crew_server_install = rejectWith("sudo wants a password");
		const { ob, t, a } = await connectStep();
		await ob.launchServer();
		await settle();
		const sudo = t.querySelector('[name="sudo-password"]');
		expect(sudo).not.toBeNull();
		expect(primary(a).textContent.trim()).toBe("Continue");
		answers.crew_server_install = () => ({});
		await typeInto(sudo, "sudo-pw");
		primary(a).click();
		await settle();
		expect(called("crew_server_install").at(-1).args.sudoPassword).toBe("sudo-pw");
		expect(ob.installDone).toBe(true);
	});
});
