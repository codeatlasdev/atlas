// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../../test/setup.ts";
import { afterEach, expect, mock, test } from "bun:test";
import { inLocale } from "../../../../test/locale.ts";

// Settings > General > Terminal host tile: the real tile over the real store. It was the second i18n proof slice, so
// every string it shows (name, status line with its plural, badges, buttons) comes from the catalog.
mock.module("@tauri-apps/api/core", () => ({ invoke: async () => null }));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
mock.module("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({}) }));
await (await import("../../../../test/uiKit.ts")).useRealKit("Button", "Spinner");
const { flushSync, mount, unmount } = await import("svelte");
const { EMPTY_HOST } = await import("#lib/hostStatus.ts");
const { hostState } = await import("#lib/stores/hostStatus.svelte.ts");
const { default: HostStatusRow } = await import("./HostStatusRow.svelte");

const pty = (id: string) => ({ id, title: "zsh", cwd: "/x", argv: ["zsh"] });
let done: (() => void) | undefined;
afterEach(() => {
	done?.();
	done = undefined;
	hostState.set(EMPTY_HOST);
	document.body.innerHTML = "";
});

function tile(state: object) {
	hostState.set({ ...EMPTY_HOST, version: "1.5.0", appVersion: "1.5.1", ...state });
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() => mount(HostStatusRow, { target }));
	done = () => unmount(app);
	flushSync();
	return {
		name: target.querySelector(".name").textContent.trim(),
		sub: target.querySelector(".tile .sub, .sub")?.textContent.trim(),
		buttons: [...target.querySelectorAll("button")].map((b) => b.textContent.trim()),
		tags: [...target.querySelectorAll(".tag")].map((b) => b.textContent.trim()),
	};
}

test("a current host reads as up to date with its terminals, singular and plural by the count, zero as none", () => {
	expect(tile({ ptys: [] })).toMatchObject({
		name: "Terminal host",
		sub: "Up to date · no running terminals",
		buttons: [],
	});
	done();
	expect(tile({ ptys: [pty("a")] }).sub).toBe("Up to date · 1 running terminal");
	done();
	expect(tile({ ptys: [pty("a"), pty("b"), pty("c")] }).sub).toBe("Up to date · 3 running terminals");
});

test("an older host names the version it is older than and offers a Restart that asks first when terminals would end", () => {
	const older = { reason: "Older than the app.", skew: "needsRestart" };
	expect(tile({ ...older, ptys: [] })).toMatchObject({
		sub: "Older than Atlas 1.5.1 · no running terminals",
		buttons: ["Restart"],
	});
	done();
	expect(tile({ ...older, ptys: [pty("a")] })).toMatchObject({
		sub: "Older than Atlas 1.5.1 · 1 running terminal",
		buttons: ["Restart…"],
	});
});

test("busy states, badges and the don't-wait choice are worded from the catalog", () => {
	expect(tile({ status: "upgrading" }).sub).toBe("Updating, terminals stay attached…");
	done();
	expect(tile({ status: "restarting" }).sub).toBe("Restarting…");
	done();
	expect(tile({ status: "outdated", idleArmed: true, reason: "x", ptys: [pty("a")] })).toMatchObject({
		tags: ["Restarts when idle"],
		buttons: ["Don't wait"],
	});
	done();
	expect(tile({ status: "outdated", deferred: true, reason: "x", ptys: [pty("a")] }).tags).toEqual([
		"Postponed",
	]);
});

test("in Brazilian Portuguese: its own plural rules (0 is none, 1 and 2 differ) and the glossary words kept", async () => {
	await inLocale("pt-BR", () => {
		expect(tile({ ptys: [] })).toMatchObject({
			name: "Host de terminais",
			sub: "Atualizado · nenhum terminal em execução",
		});
		done();
		expect(tile({ ptys: [pty("a")] }).sub).toBe("Atualizado · 1 terminal em execução");
		done();
		expect(tile({ ptys: [pty("a"), pty("b")] }).sub).toBe("Atualizado · 2 terminais em execução");
		done();
		expect(tile({ reason: "x", skew: "needsRestart", ptys: [pty("a")] })).toMatchObject({
			sub: "Mais antigo que o Atlas 1.5.1 · 1 terminal em execução",
			buttons: ["Reiniciar…"],
		});
	});
});

test("the tile follows a language switch while it is on screen", () => {
	tile({ ptys: [] });
	const root = document.body;
	expect(root.textContent).toContain("Terminal host");
	return inLocale("pt-BR", () => {
		flushSync();
		expect(root.textContent).toContain("Host de terminais");
	});
});
