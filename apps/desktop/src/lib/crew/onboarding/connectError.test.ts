// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import cases from "../../__fixtures__/crew-connect-errors.json";
import { describeConnectError, folderOf, parseConnectError, privateKeyPath } from "./connectError.ts";

// The payloads are the REAL serialization of the Rust `ConnectError`, produced by running OpenSSH's and the CLI's own
// sentences through the Rust classifier (`__fixtures__/crew-connect-errors.json`, written by
// `UPDATE_FIXTURES=1 cargo test -p atlas-desktop crew_ssh`): never hand-written JSON of what Rust "should" send.
const named = (name: string) => {
	const c = cases.find((x) => x.name === name);
	if (!c) throw new Error(`no fixture case ${name}`);
	return parseConnectError(c.error);
};

const ctx = {
	keyPath: "C:\\Users\\me\\.ssh\\id_work",
	keyAdded: false,
	target: "ubuntu@203.0.113.7",
	windows: true,
};

describe("a refused key says which key and what to do next (#517)", () => {
	test("names the key and offers the password or another key", () => {
		const f = describeConnectError(named("key refused by the server"), ctx);
		expect(f.title).toBe("The server refused the key C:\\Users\\me\\.ssh\\id_work");
		expect(f.actions).toEqual(["use_password", "pick_key"]);
		expect(f.body).toContain("password");
		// The raw last line stays available for the "Details" disclosure.
		expect(f.detail).toContain("Permission denied (publickey)");
	});

	test("after Atlas added the key itself, the advice is not 'sign in with the password' again", () => {
		const f = describeConnectError(named("key refused by the server"), { ...ctx, keyAdded: true });
		expect(f.title).toContain("still refuses");
		expect(f.actions).toEqual(["retry"]);
		expect(f.body).toContain("PubkeyAuthentication");
	});

	test("too many keys offered is the same problem", () => {
		expect(describeConnectError(named("too many keys offered"), ctx).kind).toBe("key_refused");
	});

	test("a key file with open permissions says so, per OS", () => {
		const e = named("key file permissions too open (Windows)");
		expect(describeConnectError(e, ctx).body).toContain("Properties");
		expect(describeConnectError(e, { ...ctx, windows: false }).body).toContain("chmod 600");
	});
});

describe("password sign-in failures", () => {
	test("a wrong password stays on the password method", () => {
		const f = describeConnectError(named("password refused"), ctx);
		expect(f.kind).toBe("password_refused");
		expect(f.actions).toEqual(["retry"]);
		expect(f.body).toContain("ubuntu");
	});

	test("a server that takes only keys sends the person back to the key method", () => {
		const f = describeConnectError(named("server takes only keys"), ctx);
		expect(f.kind).toBe("password_not_allowed");
		expect(f.actions).toEqual(["use_key"]);
	});
});

describe("the connection itself", () => {
	test.each([
		["host unreachable", "Cannot reach 203.0.113.7"],
		["timeout", "203.0.113.7 did not answer"],
		["name does not resolve", "The address 203.0.113.7 does not exist"],
	])("%s names the host and offers to try again", (name, title) => {
		const f = describeConnectError(named(name), ctx);
		expect(f.title).toBe(title);
		expect(f.actions).toEqual(["retry"]);
	});

	test("a changed host key gives the command that forgets the old one", () => {
		const f = describeConnectError(named("host key changed"), ctx);
		expect(f.command).toBe("ssh-keygen -R 203.0.113.7");
		expect(f.body).toContain("intercepting");
	});
});

describe("sudo", () => {
	test("sudo that wants a password asks for it below the form", () => {
		const f = describeConnectError(named("sudo wants a password"), ctx);
		expect(f.actions).toEqual(["sudo"]);
		const wrong = describeConnectError(named("sudo password wrong"), ctx);
		expect(wrong.kind).toBe("sudo_rejected");
		expect(wrong.actions).toEqual(["sudo"]);
	});
});

describe("ssh missing", () => {
	test("on Windows says which optional feature to turn on and gives the command", () => {
		const f = describeConnectError(named("ssh not installed (Windows without the OpenSSH Client)"), ctx);
		expect(f.title).toBe("OpenSSH Client is not installed");
		expect(f.body).toContain("Optional features");
		expect(f.command).toContain("Add-WindowsCapability");
		expect(f.actions).toEqual(["recheck"]);
	});

	test("elsewhere it is a package to install, not a Windows feature", () => {
		const f = describeConnectError(named("ssh not installed (Windows without the OpenSSH Client)"), {
			...ctx,
			windows: false,
		});
		expect(f.body).not.toContain("Windows");
		expect(f.command).toBeUndefined();
	});
});

describe("anything else", () => {
	// #518: the CLI writes `hint: …` for a masked unit; the screen shows it as the fix and keeps the raw error as detail.
	test("shows the CLI's own hint as the next step and the raw error under Details", () => {
		const e = named("masked unit with the CLI's hint");
		expect(e.hint).toContain("unmask atlas-crew.service");
		const f = describeConnectError(e, ctx);
		expect(f.body).toBe(e.hint);
		expect(f.detail).toContain("is masked");
	});

	test("with no hint it points at the details instead of guessing", () => {
		const f = describeConnectError(named("anything else"), ctx);
		expect(f.kind).toBe("other");
		expect(f.body).toContain("details below");
		expect(f.actions).toEqual(["retry"]);
	});
});

describe("every kind Rust produces is understood, and nothing speaks of a Mac", () => {
	test("each fixture error becomes a complete failure on both platforms", () => {
		for (const c of cases) {
			for (const windows of [true, false]) {
				const f = describeConnectError(parseConnectError(c.error), { ...ctx, windows });
				expect(f.title.length, c.name).toBeGreaterThan(0);
				expect(f.body.length, c.name).toBeGreaterThan(0);
				expect(f.actions.length, c.name).toBeGreaterThan(0);
				expect(`${f.title} ${f.body}`, c.name).not.toMatch(/\bMac(s|'s|Book)?\b/);
				expect(f.kind).toBe(c.error.kind);
			}
		}
		// A kind the screen does not know would fall into "The install stopped": only `other` may.
		for (const c of cases.filter((x) => x.error.kind !== "other")) {
			expect(describeConnectError(parseConnectError(c.error), ctx).title, c.name).not.toBe(
				"The install stopped",
			);
		}
	});

	test("an error that is not the Rust object (a string, an Error) is a plain failure with its text as detail", () => {
		expect(parseConnectError("cannot run atlas: not found")).toEqual({
			kind: "other",
			detail: "cannot run atlas: not found",
			hint: null,
		});
		expect(parseConnectError(new Error("boom")).detail).toBe("boom");
	});
});

describe("the key file picker", () => {
	test("picking the .pub means the private key next to it", () => {
		expect(privateKeyPath("/h/.ssh/id_work.pub")).toBe("/h/.ssh/id_work");
		expect(privateKeyPath("C:\\Users\\me\\.ssh\\id_work.PUB")).toBe("C:\\Users\\me\\.ssh\\id_work");
		expect(privateKeyPath("/h/.ssh/id_work")).toBe("/h/.ssh/id_work");
	});

	test("opens in the folder of the key shown, with either separator", () => {
		expect(folderOf("/h/.ssh/id_work")).toBe("/h/.ssh");
		expect(folderOf("C:\\Users\\me\\.ssh\\id_work")).toBe("C:\\Users\\me\\.ssh");
		expect(folderOf("")).toBeUndefined();
	});
});
