// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { beforeAll, describe, expect, test } from "bun:test";
import { classify } from "#lib/errors/diagnostics.ts";
import {
	hostErrorDetail,
	hostRejection,
	hostText,
	isHostFailure,
	isProgramMissing,
	loadHostMessages,
} from "#lib/hostText.ts";
import { ghAuth } from "#lib/repoState.ts";
import { HostError, render } from "#lib/hostMessages.ts";
import { inLocale } from "../../test/locale.ts";
import client from "./__fixtures__/hostd-client-errors.json";
import reply from "./__fixtures__/hostd-reply-errors.json";

beforeAll(loadHostMessages);

// The fixture is the REAL output of hostd for each failure of `fs_*` and `exec` as the desktop shell hands it to the
// webview (`UPDATE_FIXTURES=1 cargo test -p atlas-host`, `crates/atlas-host/src/protocol.rs`): nothing here is a
// hand-written copy of what the other side sends.
const sample = (code, match = {}) =>
	reply.errors.find((e) => e.code === code && Object.entries(match).every(([k, v]) => e.args[k] === v));
const pt = (w) => inLocale("pt-BR", () => render(w.code, w.args));
const ptError = (w) => inLocale("pt-BR", () => hostRejection(w));

describe("the sentence of every code of a failed fs_* or exec request", () => {
	test("the fixture covers every code hostd declares", () => {
		for (const code of reply.codes) expect(sample(code)).toBeDefined();
	});

	test.each(reply.errors.map((e) => [`${e.code} ${JSON.stringify(e.args)}`, e]))(
		"%s has a sentence in each language that is not the OS's English",
		async (_name, w) => {
			const en = render(w.code, w.args);
			const ptBr = await pt(w);
			for (const text of [en, ptBr]) {
				expect(text).toBeDefined();
				expect(text).not.toBe(w.message);
				expect(text).not.toContain("os error");
				expect(text).not.toContain("not_found");
				expect(text).not.toContain("undefined");
			}
			expect(ptBr).not.toBe(en);
		},
	);

	test("the sentence carries the path or the program, in both languages", async () => {
		for (const w of reply.errors) {
			const named = w.args.path ?? w.args.program;
			if (named === undefined) continue;
			expect(render(w.code, w.args)).toContain(named);
			expect(await pt(w)).toContain(named);
		}
	});

	// Incident: deleting a file that was already gone toasted the literal `not_found`.
	test("fs_remove of a missing file shows a Portuguese sentence that names the file, not not_found", async () => {
		const wire = sample("fs.not_found");
		expect(wire.message).toBe("not_found");
		const err = await ptError(wire);
		expect(err).toBeInstanceOf(HostError);
		expect(err.message).not.toBe("not_found");
		expect(err.message).toContain(wire.args.path);
		expect(err.message).toMatch(/Não foi possível encontrar/);
		expect(isHostFailure(err)).toBe(false);
	});

	test("a folder without permission says to check the permissions, in Portuguese", async () => {
		const text = await pt(sample("fs.io", { kind: "permission_denied" }));
		expect(text).toContain("/work/locked");
		expect(text).toMatch(/permiss/);
		expect(text).not.toContain("Permission denied");
	});

	test("a program that is not installed says to install it or check PATH, in Portuguese", async () => {
		const text = await pt(sample("exec.spawn_failed", { reason: "not_found" }));
		expect(text).toContain("git");
		expect(text).toMatch(/Instale/);
		expect(text).toContain("PATH");
		expect(text).not.toContain("No such file");
	});

	test("`other` keeps the OS's words out of the sentence and in the detail", async () => {
		for (const code of ["fs.io", "exec.spawn_failed"]) {
			const w = sample(code, code === "fs.io" ? { kind: "other" } : { reason: "other" });
			const err = await ptError(w);
			expect(err.message).not.toContain(w.args.detail);
			expect(hostErrorDetail(err)).toBe(w.args.detail);
			expect(err.message).not.toBe(w.message);
		}
		// A kind that has its own sentence has no detail to show.
		expect(hostErrorDetail(await ptError(sample("fs.io", { kind: "already_exists" })))).toBe("");
		expect(hostErrorDetail("not_found")).toBe("");
		expect(hostErrorDetail(new Error("boom"))).toBe("");
	});
});

describe("older and newer peers", () => {
	// An older hostd answers with the text alone: shown as it always was.
	test("a plain string from a daemon without the codes is still what it said", async () => {
		expect(await hostRejection("not_found")).toBe("not_found");
		expect(await hostRejection("Permission denied (os error 13)")).toBe("Permission denied (os error 13)");
		expect(hostText({ message: "not_found" })).toBe("not_found");
	});

	test("a code this build does not know shows the English of the wire", async () => {
		const wire = { code: "fs.from_the_future", args: {}, message: "Quota exceeded (os error 122)" };
		expect(render(wire.code, wire.args)).toBeUndefined();
		expect((await ptError(wire)).message).toBe(wire.message);
	});

	test("a known code whose kind or arguments are unknown shows the English instead of `undefined`", async () => {
		for (const wire of [
			{ code: "fs.io", args: { kind: "from_the_future", path: "/w" }, message: "Weird (os error 9)" },
			{ code: "fs.io", args: { kind: "permission_denied" }, message: "Permission denied (os error 13)" },
			{ code: "fs.not_found", args: {}, message: "not_found" },
			{
				code: "exec.spawn_failed",
				args: { reason: "not_found" },
				message: "git: No such file or directory (os error 2)",
			},
			{
				code: "exec.spawn_failed",
				args: { program: "git", reason: "from_the_future" },
				message: "git: Weird (os error 9)",
			},
		]) {
			expect((await ptError(wire)).message).toBe(wire.message);
		}
	});
});

describe("a missing program is told by its code, in any language", () => {
	test("exec.spawn_failed not_found is a missing program; other failures and the same OS error for a folder are not", async () => {
		const missing = await ptError(sample("exec.spawn_failed", { reason: "not_found" }));
		expect(isProgramMissing(missing)).toBe(true);
		expect(
			isProgramMissing(await ptError(sample("exec.spawn_failed", { reason: "permission_denied" }))),
		).toBe(false);
		expect(isProgramMissing(await ptError(sample("exec.spawn_failed", { reason: "other" })))).toBe(false);
		// `git status` in a folder that is gone: the OS said ENOENT too, and installing git would not help.
		expect(isProgramMissing(await ptError(sample("fs.not_found", { path: "/work/gone" })))).toBe(false);
	});

	test("an older hostd's plain text is still read by its words", () => {
		expect(isProgramMissing("gh: No such file or directory (os error 2)")).toBe(true);
		expect(isProgramMissing(new Error("gh: The system cannot find the file specified. (os error 2)"))).toBe(
			true,
		);
		expect(isProgramMissing("Permission denied (os error 13)")).toBe(false);
	});

	// Incident guard: `ghAuth` read the OS's English ("os error 2"), which a Portuguese sentence no longer contains.
	test("gh missing is `not-installed` in Portuguese too, and a refusal to run it is not", async () => {
		const missing = await ptError(sample("exec.spawn_failed", { reason: "not_found" }));
		expect(await ghAuth(async () => Promise.reject(missing))).toBe("not-installed");
		const denied = await ptError(sample("exec.spawn_failed", { reason: "permission_denied" }));
		expect(await ghAuth(async () => Promise.reject(denied))).toBe("unknown");
	});
});

describe("a busy host is a host failure by its code", () => {
	const wires = client.errors.filter((e) => ["hostd.busy", "hostd.too_many_clients"].includes(e.code));

	test("the client reports both refusals with a code of their own", () => {
		expect(wires.map((w) => w.code).sort()).toEqual(["hostd.busy", "hostd.too_many_clients"]);
		expect(client.errors.find((e) => e.code === "hostd.refused").args.why).toBe("hello timeout");
	});

	test("each is recognized in Portuguese, where the English words are gone", async () => {
		for (const w of wires) {
			const err = await ptError(w);
			expect(err).toBeInstanceOf(HostError);
			expect(err.message).not.toBe(w.message);
			// The regex never matched these two, and in Portuguese it could not.
			expect(classify(err.message)).toBe("other");
			expect(isHostFailure(err)).toBe(true);
		}
	});

	test("a refusal that is not about load is not one", async () => {
		const timeout = client.errors.find((e) => e.code === "hostd.refused");
		expect(isHostFailure(await ptError(timeout))).toBe(false);
	});
});
