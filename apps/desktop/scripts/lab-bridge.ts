/**
 * Vite dev-server side of Atlas Lab (see src/lib/debug/labShim.ts). Active only when
 * ATLAS_DEBUG_SOCK names an agent instance (`bun run desktop:lab` sets it); it never falls back
 * to the user's own app socket.
 *
 * `/__lab/eval` runs code in that instance with its full IPC, so only this page may call it:
 * a per-run token readable same-origin only (no CORS headers), an Origin that must be this dev
 * server, and a JSON content type, which forces a preflight cross-origin.
 */
import { randomBytes } from "node:crypto";
import { connect } from "node:net";
import type { Plugin } from "vite";

export function bridgeCall(
	sock: string,
	req: object,
	timeoutMs = 15_000,
): Promise<{ ok: boolean; result?: unknown; error?: string }> {
	const { promise, resolve } = Promise.withResolvers<{ ok: boolean; result?: unknown; error?: string }>();
	const s = connect(sock);
	let buf = "";
	const done = (v: { ok: boolean; result?: unknown; error?: string }) => {
		s.destroy();
		resolve(v);
	};
	const timer = setTimeout(
		() => done({ ok: false, error: "lab bridge: instance did not answer" }),
		timeoutMs,
	);
	s.on("connect", () => s.write(`${JSON.stringify(req)}\n`));
	s.on("data", (d) => {
		buf += d;
		const nl = buf.indexOf("\n");
		if (nl >= 0) {
			clearTimeout(timer);
			try {
				done(JSON.parse(buf.slice(0, nl)));
			} catch (e) {
				done({ ok: false, error: `lab bridge: bad reply: ${e}` });
			}
		}
	});
	s.on("error", (e) => {
		clearTimeout(timer);
		done({ ok: false, error: `lab bridge: ${e.message} (is the agent instance running?)` });
	});
	return promise;
}

/** Same-origin check: the Origin header must name the host the request was sent to. */
export function sameOrigin(origin: string | undefined, host: string | undefined): boolean {
	if (!origin || !host) return false;
	try {
		const u = new URL(origin);
		return (u.protocol === "http:" || u.protocol === "https:") && u.host === host;
	} catch {
		return false;
	}
}

/** Whether a `/__lab/eval` request comes from this page: same Origin, the run's token, JSON body. */
export function allowed(h: Record<string, string | string[] | undefined>, token: string): boolean {
	const one = (k: string) => (typeof h[k] === "string" ? (h[k] as string) : undefined);
	return (
		sameOrigin(one("origin"), one("host")) &&
		one("x-atlas-lab") === token &&
		(one("content-type")?.startsWith("application/json") ?? false)
	);
}

export function labBridge(): Plugin {
	const sock = process.env.ATLAS_DEBUG_SOCK;
	const instance = process.env.ATLAS_LAB_INSTANCE ?? "agent";
	const token = randomBytes(24).toString("hex");
	return {
		name: "atlas-lab-bridge",
		apply: "serve",
		config() {
			// The bridge runs code in the instance: never listen beyond this machine.
			if (sock) return { server: { host: "127.0.0.1" } };
		},
		async configureServer(server) {
			if (!sock) return;
			const info = await bridgeCall(sock, { cmd: "info" }, 5000);
			const agent = (info.result as { agent?: boolean } | undefined)?.agent;
			if (!info.ok || agent !== true) {
				server.config.logger.error(
					`[atlas-lab] ${sock} is not an agent instance (${info.error ?? "agent: false"}); bridge off`,
				);
				return;
			}
			server.config.logger.info(`[atlas-lab] backend: agent instance ${instance} (${sock})`);
			server.middlewares.use("/__lab", (req, res) => {
				const send = (code: number, body: unknown) => {
					res.statusCode = code;
					res.setHeader("content-type", "application/json");
					res.end(JSON.stringify(body));
				};
				if (req.method === "GET" && req.url === "/token") {
					// Same-origin fetch only: without CORS headers another site cannot read it.
					return send(200, { token, instance });
				}
				if (req.method !== "POST" || req.url !== "/eval") return send(404, { ok: false, error: "not found" });
				if (!allowed(req.headers, token)) {
					return send(403, { ok: false, error: "forbidden" });
				}
				let body = "";
				req.on("data", (d) => (body += d));
				req.on("end", async () => {
					let js: unknown;
					try {
						js = JSON.parse(body).js;
					} catch {
						return send(400, { ok: false, error: "bad json" });
					}
					if (typeof js !== "string") return send(400, { ok: false, error: "missing js" });
					send(200, await bridgeCall(sock, { cmd: "eval", js }));
				});
			});
		},
	};
}
