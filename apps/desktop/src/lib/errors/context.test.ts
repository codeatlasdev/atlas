// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { gatherContext, osLabel } from "./context.ts";

test("osLabel reads macOS and the CPU from the user agent without throwing on junk", () => {
	expect(osLabel("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15")).toBe("macOS");
	expect(osLabel("Mozilla/5.0 (X11; Linux x86_64)")).toBe("Linux");
	expect(osLabel("")).toBeUndefined();
});

test("gathers what answers and silently skips what does not (a crashed app must still report)", async () => {
	const ctx = await gatherContext({
		invoke: async (cmd) => {
			if (cmd === "host_status") return { version: "0.6.2", proto: 1, caps: ["pty_attach_raw"] };
			throw new Error("no system_info in the tray");
		},
		nav: { userAgent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)", language: "pt-BR" },
		route: "/",
		crumbs: [{ at: 1, text: "x" }],
	});
	expect(ctx.hostVersion).toBe("0.6.2");
	expect(ctx.hostCaps).toEqual(["pty_attach_raw"]);
	expect(ctx.locale).toBe("pt-BR");
	expect(ctx.os).toBe("macOS");
	expect(ctx.breadcrumbs).toEqual([{ at: 1, text: "x" }]);
});

test("a hung backend does not hang the error screen", async () => {
	const asked: string[] = [];
	const ctx = await gatherContext({
		invoke: (cmd) => (asked.push(cmd), new Promise(() => {})),
		nav: { userAgent: "", language: "en" },
		route: "/",
		crumbs: [],
		timeoutMs: 50,
	});
	// Resolving at all is the guard: a call that never answers is given up on. (A `< 500 ms` wall-clock assert failed on a
	// loaded machine with nothing regressed; a regression here is a test that never finishes, which the suite's per-test
	// timeout turns into a failure.)
	expect(asked).toEqual(["host_status", "host_diagnostics"]);
	expect(ctx.hostVersion).toBeUndefined();
	expect(ctx.diagnostics).toBeUndefined();
});

test("the report core's diagnostics ride along when the backend answers, and are skipped when it does not", async () => {
	const diag = {
		versions: {
			app: "1.5.0-alpha.5",
			cli: "1.5.0-alpha.5",
			hostd: "1.5.0-alpha.4",
			hostdPid: 7,
			hostdCaps: [],
		},
	};
	const ok = await gatherContext({
		invoke: async (cmd) => (cmd === "host_diagnostics" ? diag : null),
		nav: { userAgent: "", language: "en" },
		route: "/",
		crumbs: [],
	});
	expect(ok.diagnostics?.versions.hostd).toBe("1.5.0-alpha.4");
	expect(ok.diagnostics?.incidents).toEqual([]);
	const down = await gatherContext({
		invoke: async () => {
			throw new Error("no backend");
		},
		nav: { userAgent: "", language: "en" },
		route: "/",
		crumbs: [],
	});
	expect(down.diagnostics).toBeUndefined();
});
