/**
 * Atlas Lab: run the desktop frontend in an ordinary browser tab (Vite, HMR) against the real
 * backend of an isolated agent instance. Every `invoke()` and every event listener is forwarded
 * through the instance's debug bridge (`scripts/lab-bridge.ts` → `ATLAS_DEBUG_SOCK` → `eval`),
 * so no Rust command is mocked. Dev only: `hooks.client.ts` loads it when the page is not inside
 * Tauri, and `import.meta.env.DEV` keeps it out of the bundle.
 *
 * Not forwarded: `Channel` arguments (terminal streams, gestures) and `convertFileSrc` assets.
 * Those surfaces need the app itself (`bun run desktop:agent`).
 */

type Callback = (payload: unknown) => void;

/** Instance-side code for one invoke. Command and args travel as JSON literals, never as code. */
export function invokeBody(cmd: string, args: unknown): string {
	return `return await window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args ?? {})});`;
}

/**
 * Instance-side code that registers a listener whose events queue up for this browser client
 * under the browser's own handler id; `drainBody` hands them over.
 */
export function listenBody(client: string, handler: number, args: Record<string, unknown>): string {
	const c = JSON.stringify(client);
	return `const L = (window.__atlasLab ??= {}); const q = (L[${c}] ??= []);
const h = window.__TAURI_INTERNALS__.transformCallback((e) => q.push({ h: ${handler}, e }));
return await window.__TAURI_INTERNALS__.invoke("plugin:event|listen", { ...${JSON.stringify(args)}, handler: h });`;
}

/** Long-poll: resolves with the queued events as soon as there are any, or [] after `waitMs`. */
export function drainBody(client: string, waitMs: number): string {
	const c = JSON.stringify(client);
	return `const L = (window.__atlasLab ??= {}); const q = (L[${c}] ??= []);
const end = Date.now() + ${waitMs};
while (q.length === 0 && Date.now() < end) await new Promise((r) => setTimeout(r, 50));
return q.splice(0);`;
}

/** Delivers drained events to the browser's callbacks; returns how many had no live callback. */
export function deliver(events: { h: number; e: unknown }[], callbacks: Map<number, Callback>): number {
	let dropped = 0;
	for (const { h, e } of events) {
		const cb = callbacks.get(h);
		if (cb) cb(e);
		else dropped++;
	}
	return dropped;
}

async function bridge(token: string, js: string): Promise<unknown> {
	const r = await fetch("/__lab/eval", {
		method: "POST",
		headers: { "content-type": "application/json", "x-atlas-lab": token },
		body: JSON.stringify({ js }),
	});
	const out = (await r.json()) as { ok: boolean; result?: unknown; error?: string };
	if (!out.ok) throw new Error(out.error ?? `lab bridge: HTTP ${r.status}`);
	return out.result;
}

export async function installLabShim(): Promise<void> {
	const info = await fetch("/__lab/token").then(
		(r) => (r.ok ? r.json() : null),
		() => null,
	);
	if (!info?.token) return; // plain `vite dev` without a lab instance: the page stays as it was
	const token: string = info.token;
	const client = crypto.randomUUID();
	const callbacks = new Map<number, Callback>();
	let next = 1;

	const w = window as unknown as Record<string, unknown>;
	w.__ATLAS_LAB__ = { instance: info.instance };
	w.__TAURI_INTERNALS__ = {
		metadata: { currentWindow: { label: "main" }, currentWebview: { windowLabel: "main", label: "main" } },
		transformCallback(cb: Callback, once = false) {
			const id = next++;
			callbacks.set(id, once ? (p) => (callbacks.delete(id), cb(p)) : cb);
			return id;
		},
		unregisterCallback(id: number) {
			callbacks.delete(id);
		},
		async invoke(cmd: string, args: Record<string, unknown> = {}) {
			if (cmd === "plugin:event|listen") {
				const { handler, ...rest } = args as { handler: number };
				return bridge(token, listenBody(client, handler, rest));
			}
			return bridge(token, invokeBody(cmd, args));
		},
		convertFileSrc(path: string) {
			return path;
		},
	};
	w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };

	(async function pump() {
		for (;;) {
			try {
				deliver((await bridge(token, drainBody(client, 8000))) as { h: number; e: unknown }[], callbacks);
			} catch (e) {
				console.warn("[atlas-lab] event pump:", e);
				await new Promise((r) => setTimeout(r, 1000));
			}
		}
	})();
	console.info(`[atlas-lab] backend: agent instance ${info.instance}`);
}
