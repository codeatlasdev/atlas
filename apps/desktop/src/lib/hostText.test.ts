// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { beforeAll, describe, expect, test } from "bun:test";
import { classify } from "#lib/errors/diagnostics.ts";
import { hostReason, parseHostState } from "#lib/hostStatus.ts";
import { hostRejection, hostText, isHostFailure, loadHostMessages, parseWire } from "#lib/hostText.ts";
import { HostError, render } from "#lib/hostMessages.ts";
import { lspReasonText } from "#lib/lsp/reasonText.ts";
import { inLocale } from "../../test/locale.ts";
import client from "./__fixtures__/hostd-client-errors.json";
import wire from "./__fixtures__/hostd-wire.json";
import lsp from "./__fixtures__/lsp-reasons.json";

// The messages load on demand in the app; the tests that render them load them first.
beforeAll(loadHostMessages);

// The fixtures are the REAL output of the Rust producers (`UPDATE_FIXTURES=1 cargo test -p atlas-lsp -p atlas-host
// -p atlas-desktop`): the status of `state_for`, the toasts and errors of `host.rs`, the coded errors of `client.rs`,
// the `Failure`s of `atlas-lsp`. Nothing here is a hand-written copy of what the other side sends.

const codedWire = [
	...client.errors,
	...wire.errors.filter((e) => e.code),
	...wire.rejections.filter((e) => typeof e === "object" && e.code),
	...wire.toasts.map((t) => ({ code: t.code, args: t.args, message: t.text })),
	...wire.status
		.filter((s) => s.reasonCode)
		.map((s) => ({ code: s.reasonCode, args: s.reasonArgs, message: s.reason })),
];

const allCodes = [...new Set([...wire.codes, ...client.codes])];

describe("every code the Rust side declares has a message", () => {
	test.each(allCodes)("%s renders from the catalog, with the English of the wire in English", (code) => {
		const samples = codedWire.filter((w) => w.code === code);
		expect(samples.length).toBeGreaterThan(0);
		for (const s of samples) {
			const text = render(code, s.args);
			expect(text).toBeDefined();
			// The English a client that does not know the code shows is the English this build shows.
			expect(text).toBe(s.message);
		}
	});

	// `hostd: {why}` is a process name and a protocol word in both languages: the one message that does not change.
	test.each(allCodes.filter((c) => c !== "hostd.refused"))(
		"%s has Portuguese that is not the English",
		async (code) => {
			for (const s of codedWire.filter((w) => w.code === code)) {
				const pt = await inLocale("pt-BR", () => render(code, s.args));
				expect(pt).toBeDefined();
				expect(pt).not.toBe(s.message);
			}
		},
	);

	test("a code this build does not know shows the English of the wire", () => {
		expect(hostText({ code: "hostd.from_the_future", args: {}, message: "English" })).toBe("English");
		expect(render("hostd.from_the_future")).toBeUndefined();
	});

	test("a code whose argument is missing (an older shell or daemon) shows the English instead of `undefined`", () => {
		for (const s of codedWire.filter(
			(w) => w.args && Object.keys(w.args).length > 0 && w.code !== "hostd.unreachable",
		)) {
			const text = hostText({ code: s.code, args: {}, message: s.message });
			expect(text).toBe(s.message);
			expect(text).not.toContain("undefined");
		}
	});
});

describe("the sentences carry the values of the wire in both languages", () => {
	test("hostd.older names the versions and the features that are off", async () => {
		const older = wire.status.find((s) => s.reasonCode && s.reasonArgs.missing.length === 3);
		const pt = await inLocale("pt-BR", () => hostReason(older));
		expect(pt).toContain("0.6.2");
		expect(pt).toContain("9.9.9");
		expect(pt).toContain("recursos de linguagem");
		expect(pt).toContain("renderização rápida de terminais");
		expect(pt).toContain("status dos agentes");
		// Only the version differs: no feature clause.
		const only = wire.status.find((s) => s.reasonCode && s.reasonArgs.missing.length === 0);
		expect(hostReason(only)).toBe(only.reason);
		expect(await inLocale("pt-BR", () => hostReason(only))).not.toContain("ficam desativados");
	});

	test("hostd.unreachable keeps the cause in parentheses", async () => {
		const withWhy = wire.errors.find((e) => e.code === "hostd.unreachable" && e.args.why);
		expect(await inLocale("pt-BR", () => hostText(withWhy))).toEndWith("(socket in use)");
	});

	test("the install command of hostd.cli_not_found follows the OS of the message", () => {
		const e = client.errors.find((x) => x.code === "hostd.cli_not_found");
		expect(render(e.code, e.args)).toContain(e.args.command);
	});
});

describe("a status from a shell without the codes", () => {
	test("keeps showing its English reason", () => {
		const before = { ...wire.status[0] };
		delete before.reasonCode;
		delete before.reasonArgs;
		const s = parseHostState(before);
		expect(s.reasonCode).toBe("");
		expect(hostReason(s)).toBe(before.reason);
	});

	test("a current host has no reason in any language", () => {
		const cur = parseHostState(wire.status.find((s) => !s.reason));
		expect(hostReason(cur)).toBe("");
	});

	test("the new keys are read from the real payload", () => {
		const s = parseHostState(wire.status[0]);
		expect(s.reasonCode).toBe("hostd.older");
		expect(s.reasonArgs.version).toBe("0.6.2");
	});
});

describe("a coded failure of the host is a host failure in any language", () => {
	// The bug class: `classify` matches English words, so a rejection shown in pt-BR stopped folding into the one
	// outage toast with its Restart action.
	test("a rejected host_call rendered in pt-BR is recognized by its code, not by its words", async () => {
		const raw = wire.rejections.find((r) => typeof r === "object" && r.code === "hostd.unreachable");
		const err = await inLocale("pt-BR", () => hostRejection(raw));
		expect(err).toBeInstanceOf(HostError);
		expect(err.message).not.toBe(raw.message);
		expect(classify(err.message)).toBe("other");
		expect(isHostFailure(err)).toBe(true);
	});

	// Regression (this review): `isHostFailure` answered true for every `hostd.*` code, so a remote crew server that
	// was still connecting ("ssh:box is still connecting") raised the LOCAL outage toast with "Restart terminal host".
	// Before the codes `classify` read those sentences as ordinary errors, and only the local host's own outage as
	// a host failure.
	test("only a failure of the local host's connection is a host failure; a remote or a refusal is the caller's", async () => {
		const outage = ["hostd.unreachable", "hostd.closed", "hostd.disconnected", "hostd.start_failed"];
		const rest = codedWire.filter(
			(w) => w.code.startsWith("hostd.") && !outage.includes(w.code) && w.message,
		);
		expect(rest.map((w) => w.code)).toContain("hostd.remote_connecting");
		expect(rest.map((w) => w.code)).toContain("hostd.remote_unreachable");
		for (const code of outage) {
			const w = codedWire.find((x) => x.code === code);
			expect(isHostFailure(await hostRejection(w))).toBe(true);
		}
		for (const w of rest) {
			// The English the old `classify` judged, and the coded failure the new code judges, agree.
			expect(classify(w.message)).toBe("other");
			expect(isHostFailure(await hostRejection(w))).toBe(false);
		}
		// hostd refusing the token on a raw attach was a host failure by its words ("hostd: unauthorized") and stays one.
		expect(
			isHostFailure(
				await hostRejection({
					code: "hostd.refused",
					args: { why: "unauthorized" },
					message: "hostd: unauthorized",
				}),
			),
		).toBe(true);
		expect(isHostFailure("Terminal host not reachable. Use Restart terminal host")).toBe(true);
		expect(isHostFailure("no such file")).toBe(false);
		// An uncoded rejection stays what it was: the same string.
		const text = wire.rejections.find((r) => typeof r === "string");
		expect(await hostRejection(text)).toBe(text);
	});

	test("a payload of host://error is an object, or a bare string from an older shell", () => {
		expect(parseWire("hostd: boom")).toEqual({ message: "hostd: boom" });
		expect(hostText(parseWire(wire.errors[0]))).toBe(wire.errors[0].message);
		expect(parseWire(42)).toBeNull();
	});
});

describe("language server reasons", () => {
	test.each(lsp.reasons.map((r) => [`${r.code} ${JSON.stringify(r.args)}`, r]))(
		"%s: message and hint render from the catalog, English equal to the daemon's",
		(_name, r) => {
			const t = lspReasonText(r);
			expect(t.message).toBe(r.message);
			expect(t.hint).toBe(r.hint);
		},
	);

	test.each(lsp.reasons.map((r) => [`${r.code} ${JSON.stringify(r.args)}`, r]))(
		"%s has Portuguese",
		async (_name, r) => {
			const t = await inLocale("pt-BR", () => lspReasonText(r));
			expect(t.message).not.toBe(r.message);
			expect(t.hint).not.toBe(r.hint);
			for (const v of Object.values(r.args))
				if (v !== "repeated") expect(t.message + (t.hint ?? "")).toContain(String(v));
		},
	);

	test("every code of atlas-lsp has a fixture entry", () => {
		const codes = new Set(lsp.reasons.map((r) => r.code));
		for (const c of [
			"no_go",
			"no_curl",
			"offline",
			"unsupported_platform",
			"install_failed",
			"custom_not_found",
			"crashed",
		])
			expect(codes.has(c)).toBe(true);
	});

	// Daemon from before `args`: the same Failure without the key (the Rust test `args_is_the_only_key_added_to_a_failure`
	// pins that `args` is the only addition). It keeps showing its English, never `undefined`.
	test("a daemon without args keeps its English message and hint", async () => {
		for (const r of lsp.reasons) {
			const old = { ...r };
			delete old.args;
			expect(lspReasonText(old)).toEqual({ message: r.message, hint: r.hint });
			expect(await inLocale("pt-BR", () => lspReasonText(old))).toEqual({ message: r.message, hint: r.hint });
		}
	});

	// A client from before `args` reads `code`, `message`, `hint` and `command`: they are all still there.
	test("an older desktop still finds the four keys it reads, with the types it reads", () => {
		for (const r of lsp.reasons) {
			expect(typeof r.code).toBe("string");
			expect(typeof r.message).toBe("string");
			expect(r.hint === undefined || typeof r.hint === "string").toBe(true);
			expect(r.command === undefined || typeof r.command === "string").toBe(true);
			expect(Object.keys(r).every((k) => ["code", "message", "hint", "command", "args"].includes(k))).toBe(
				true,
			);
		}
	});

	test("an internal crash reason with no args shows its message as it is", () => {
		expect(lspReasonText({ code: "crashed", message: "idle" })).toEqual({ message: "idle", hint: undefined });
	});
});
