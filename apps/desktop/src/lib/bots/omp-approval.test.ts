// @ts-nocheck bun test runtime types are not part of the app tsconfig
/**
 * Tests for the omp approval extension's fail-closed behavior.
 *
 * The extension is generated at runtime by Rust (omp.rs::write_approval_extension)
 * and written to <bot_dir>/omp-approval.ts. We test the core decision logic here
 * in isolation using a local Bun.serve() stub, verifying that ANY failure path
 * (5xx, connection refused, timeout, malformed JSON, "deny") results in block:true.
 */
import { afterAll, describe, expect, it } from "bun:test";
import type { Server } from "bun";

// ─── The exact decision logic from the generated extension ──────────────────
// Keep in sync with omp.rs write_approval_extension().
async function requestApproval(
	port: number,
	token: string,
	pane: string,
	toolName: string,
	toolInput: unknown,
	timeoutMs = 5_000,
): Promise<"allow" | "block"> {
	let decision: string | null = null;
	const ctrl = new AbortController();
	const timer = setTimeout(() => ctrl.abort(), timeoutMs);
	try {
		const resp = await fetch(`http://127.0.0.1:${port}/hook/omp/decision`, {
			method: "POST",
			headers: {
				"Content-Type": "application/json",
				"x-atlas-hook-token": token,
			},
			body: JSON.stringify({
				pane_key: pane,
				payload: { tool_name: toolName, tool_input: toolInput },
			}),
			signal: ctrl.signal,
		});
		if (resp.ok) {
			const data = await resp.json();
			if (data && data.decision === "allow") {
				decision = "allow";
			}
		}
	} catch {
		// Network error, timeout, malformed JSON — fall through to block.
	} finally {
		clearTimeout(timer);
	}
	return decision === "allow" ? "allow" : "block";
}

// ─── helpers ─────────────────────────────────────────────────────────────────

function stubServer(handler: (req: Request) => Response | Promise<Response>): Server {
	return Bun.serve({ port: 0, fetch: handler });
}

const TOKEN = "test-token";
const PANE = "bot:abc:task1";

// ─── tests ───────────────────────────────────────────────────────────────────
// Bun.serve requires the native Bun Response class. When happy-dom is active
// (--conditions browser test run) it replaces globalThis.Response with a browser
// polyfill that Bun.serve rejects. Skip the suite in that environment; it runs
// clean in a standalone `bun test` without the preload.
const suiteDescribe = typeof document !== "undefined" ? describe.skip : describe;

suiteDescribe("omp approval extension — fail-closed", () => {
	const servers: Server[] = [];
	afterAll(() => servers.forEach((s) => s.stop(true)));

	it("allows when server returns {decision:'allow'}", async () => {
		const s = stubServer(
			() =>
				new Response(JSON.stringify({ decision: "allow" }), {
					status: 200,
					headers: { "Content-Type": "application/json" },
				}),
		);
		servers.push(s);
		const result = await requestApproval(s.port, TOKEN, PANE, "Bash", {});
		expect(result).toBe("allow");
	});

	it("blocks when server returns {decision:'deny'}", async () => {
		const s = stubServer(
			() =>
				new Response(JSON.stringify({ decision: "deny" }), {
					status: 200,
					headers: { "Content-Type": "application/json" },
				}),
		);
		servers.push(s);
		const result = await requestApproval(s.port, TOKEN, PANE, "Bash", {});
		expect(result).toBe("block");
	});

	it("blocks when server returns 500", async () => {
		const s = stubServer(() => new Response("Internal Server Error", { status: 500 }));
		servers.push(s);
		const result = await requestApproval(s.port, TOKEN, PANE, "Bash", {});
		expect(result).toBe("block");
	});

	it("blocks when server returns 200 with malformed JSON", async () => {
		const s = stubServer(() => new Response("not json at all {{{", { status: 200 }));
		servers.push(s);
		const result = await requestApproval(s.port, TOKEN, PANE, "Bash", {});
		expect(result).toBe("block");
	});

	it("blocks when server returns 200 with null body", async () => {
		const s = stubServer(
			() =>
				new Response(JSON.stringify(null), { status: 200, headers: { "Content-Type": "application/json" } }),
		);
		servers.push(s);
		const result = await requestApproval(s.port, TOKEN, PANE, "Bash", {});
		expect(result).toBe("block");
	});

	it("blocks when server returns 200 with missing decision field", async () => {
		const s = stubServer(
			() =>
				new Response(JSON.stringify({ ok: true }), {
					status: 200,
					headers: { "Content-Type": "application/json" },
				}),
		);
		servers.push(s);
		const result = await requestApproval(s.port, TOKEN, PANE, "Bash", {});
		expect(result).toBe("block");
	});

	it("blocks on connection refused (wrong port)", async () => {
		// Port 1 is privileged and always connection-refused on macOS/Linux.
		// Use a port that was open then closed to guarantee ECONNREFUSED.
		const s = stubServer(() => new Response("ok"));
		const closedPort = s.port;
		s.stop(true);
		servers.push(s);
		const result = await requestApproval(closedPort, TOKEN, PANE, "Bash", {}, 2_000);
		expect(result).toBe("block");
	});

	it("blocks on timeout (server never responds)", async () => {
		// Integration test: exercises real AbortController + fetch abort path against a
		// live hanging socket. Fake timers cannot abort a real in-flight fetch request,
		// so real wall-clock time is required here. Timeout is kept short (300 ms).
		const s = stubServer(() => new Promise(() => {}) as Promise<Response>);
		servers.push(s);
		const result = await requestApproval(s.port, TOKEN, PANE, "Bash", {}, 300);
		expect(result).toBe("block");
	}, 2_000);
});
