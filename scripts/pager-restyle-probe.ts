// bun scripts/pager-restyle-probe.ts [--nodes 23000] [--budget-ms 10] [--layer <SpaceLayer.svelte>] [--chrome <path>] [--json]
//
// What a space switch costs the browser's STYLE engine, in a real Chromium, with the real `SpacePager.svelte`
// stylesheet: it compiles the component, mounts the layer roots the component renders (`data-role`, `inert`,
// `aria-hidden`, `data-side`, the same classes) around ~23k-node subtrees (the size of one 400-line Changes tab),
// then applies exactly the attribute flips a commit does and reads Chromium's own `RecalcStyleDuration` /
// `RecalcStyleCount` / `UpdateLayoutTree.elementCount` around a forced style+layout.
//
// Why it exists (#121): after #115 a switch no longer remounts or refetches, but flipping the roles of the two layer
// roots restyled every node of both layers (~160 ms for 2 x 23k nodes, 286 ms in the app with 5 loaded spaces). The
// culprit is not a selector: `pointer-events` and `inert` are INHERITED, so a change on the root recomputes the
// style of every descendant. The probe exits 1 when a flip restyles more than a handful of elements or costs more
// than the budget. Needs no app instance and no Screen Recording permission (the app's hidden agent pages cannot
// sample rAF); `scripts/selection-bench.ts` and `gesture-baseline.ts` measure the same switch inside the real app.
import { spawn } from "bun";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { compile } from "svelte/compiler";

const arg = (k: string, d?: string) => {
	const i = process.argv.indexOf(`--${k}`);
	return i >= 0 ? process.argv[i + 1] : d;
};

/** `SpaceLayer.svelte` compiled for the browser (its CSS, Svelte scoping included) and for the server (its markup). */
export function compileLayer() {
	// `--layer <file>` judges another version of the component (the design before a fix) with the same probe
	const file =
		arg("layer") ??
		join(import.meta.dir, "../apps/desktop/src/routes/(app)/SpaceLayer.svelte");
	const src = readFileSync(file, "utf8");
	const client = compile(src, { filename: file, generate: "client" });
	const server = compile(src, { filename: file, generate: "server" });
	return { css: client.css?.code ?? "", server: server.js.code };
}

/**
 * The attributes a role flip changes, read from the real component: render `SpaceLayer` as current and as near
 * (server-side, with a marker child) and diff the two element lists (root and body), so the probe applies exactly what
 * the component changes and nothing a hand-written list could drift from.
 */
export async function renderLayer(current: boolean): Promise<string> {
	const { css, server } = compileLayer();
	// next to the app's `svelte` install, so the compiled module resolves `svelte/internal/server`
	const dir = mkdtempSync(
		join(import.meta.dir, "../apps/desktop/.pager-probe-"),
	);
	try {
		const file = join(dir, "SpaceLayer.mjs");
		await Bun.write(file, server);
		const { render } = await import("svelte/server");
		const mod = (await import(file)) as { default: unknown };
		const vars = { accent: "red", text: "red", onAccent: "white", tint: "red" };
		const children = (r: { push(s: string): void }) => r.push("@@BODY@@");
		const out = render(mod.default as never, {
			props: {
				id: "s",
				current,
				side: "next",
				vars,
				layer: () => ({ destroy() {} }),
				children,
			},
		});
		void css;
		return out.body;
	} finally {
		rmSync(dir, { recursive: true, force: true });
	}
}

/** Content shaped like a Changes tab: a table of rows of spans, a button and an svg, ~16 nodes per row. */
const CONTENT_CSS = `.row{display:table-row}.g{color:#888;text-align:right;padding:0 6px;user-select:none;font:11px monospace}
.c{font:12px/1.4 monospace;white-space:pre}.t{color:#333}.t0{color:#a00}.t1{color:#0a0}.t2{color:#00a}.t3{color:#a0a}
.b{cursor:pointer;border:0;background:none;padding:0;opacity:.5}.row:hover .b{opacity:1}.i{fill:none;stroke:currentColor}`;
const row = (i: number) =>
	`<tr class="row"><td class="g">${i}</td><td class="g">${i}</td><td class="c">${[0, 1, 2, 3, 4, 5, 6, 7].map((k) => `<span class="t t${k % 4}">tok</span>`).join("")}<button class="b"><svg class="i" width="12" height="12"><path d="M1 1L9 9"/><path d="M9 1L1 9"/></svg></button></td></tr>`;
const content = (nodes: number) =>
	`<table>${Array.from({ length: Math.ceil(nodes / 16) }, (_, i) => row(i)).join("")}</table>`;

type Cost = {
	recalcMs: number;
	recalcs: number;
	elements: number;
	wallMs: number;
};

async function main() {
	const nodes = Number(arg("nodes", "23000"));
	const budgetMs = Number(arg("budget-ms", "10"));
	const chrome =
		arg("chrome") ??
		process.env.CHROME ??
		(await Bun.$`ls -d ${process.env.HOME}/Library/Caches/ms-playwright/chromium-*/chrome-mac-arm64/*.app/Contents/MacOS/*`
			.text()
			.then((s) => s.split("\n")[0]));
	if (!chrome)
		throw new Error("no Chromium: pass --chrome <path> or set CHROME");

	const { css } = compileLayer();
	const [current, near] = [await renderLayer(true), await renderLayer(false)];
	const html = `<!doctype html><meta charset=utf8><style>html,body{margin:0;height:100%}
.pager{position:relative;flex:1;min-height:0;min-width:0;overflow:hidden;width:1264px;height:774px}
${CONTENT_CSS}${css}</style><body><div class="pager" id="pager"></div></body>`;

	const dir = mkdtempSync(join(tmpdir(), "pager-probe-"));
	const port = 9400 + Math.floor(Math.random() * 400);
	const proc = spawn(
		[
			chrome,
			`--remote-debugging-port=${port}`,
			"--headless=new",
			"--no-first-run",
			"--disable-gpu",
			`--user-data-dir=${dir}`,
			"--window-size=1264,774",
			"about:blank",
		],
		{ stdout: "ignore", stderr: "ignore" },
	);
	try {
		let ws: string | undefined;
		for (let i = 0; i < 100 && !ws; i++) {
			try {
				const t = (await (
					await fetch(`http://127.0.0.1:${port}/json`)
				).json()) as {
					type: string;
					webSocketDebuggerUrl: string;
				}[];
				ws = t.find((x) => x.type === "page")?.webSocketDebuggerUrl;
			} catch {}
			if (!ws) await Bun.sleep(100);
		}
		if (!ws) throw new Error("Chromium did not start");
		const sock = new WebSocket(ws);
		const opened = Promise.withResolvers<void>();
		sock.onopen = () => opened.resolve();
		await opened.promise;
		type Event = { method?: string; params?: { value?: unknown[] } };
		type Reply = { id?: number; result?: unknown; error?: unknown } & Event;
		let id = 0;
		const pending = new Map<number, (m: Reply) => void>();
		const listeners = new Set<(m: Event) => void>();
		sock.onmessage = (e) => {
			const m = JSON.parse(String(e.data)) as Reply;
			const answer = m.id === undefined ? undefined : pending.get(m.id);
			if (answer) {
				pending.delete(m.id!);
				answer(m);
			} else for (const l of listeners) l(m);
		};
		/** One CDP call; the reply's shape depends on the method, so the caller names what it reads. */
		const send = <T = Record<string, unknown>>(
			method: string,
			params: object = {},
		) => {
			const { promise, resolve, reject } = Promise.withResolvers<T>();
			const i = ++id;
			pending.set(i, (m) =>
				m.error
					? reject(new Error(JSON.stringify(m.error)))
					: resolve(m.result as T),
			);
			sock.send(JSON.stringify({ id: i, method, params }));
			return promise;
		};
		const evalJs = async (expression: string) => {
			const r = await send<{
				result: { value: unknown };
				exceptionDetails?: unknown;
			}>("Runtime.evaluate", {
				expression,
				awaitPromise: true,
				returnByValue: true,
			});
			if (r.exceptionDetails)
				throw new Error(JSON.stringify(r.exceptionDetails).slice(0, 400));
			return r.result.value;
		};
		await send("Page.enable");
		await send("Runtime.enable");
		await send("Performance.enable");
		await send("Page.navigate", {
			url: `data:text/html;charset=utf8,${encodeURIComponent(html)}`,
		});
		for (let i = 0; i < 100; i++) {
			if ((await evalJs(`document.readyState`)) === "complete") break;
			await Bun.sleep(50);
		}

		const metric = async () =>
			Object.fromEntries(
				(
					await send<{ metrics: { name: string; value: number }[] }>(
						"Performance.getMetrics",
					)
				).metrics.map((m) => [m.name, m.value]),
			);
		/** Elements Chromium restyled in one forced update: the `UpdateLayoutTree` trace event's `elementCount`. */
		const traced = async (js: string) => {
			const events: { name: string; args?: { elementCount?: number } }[] = [];
			const complete = Promise.withResolvers<void>();
			const listen = (m: Event) => {
				if (m.method === "Tracing.dataCollected")
					events.push(...(m.params!.value as typeof events));
				if (m.method === "Tracing.tracingComplete") complete.resolve();
			};
			listeners.add(listen);
			await send("Tracing.start", {
				traceConfig: { includedCategories: ["devtools.timeline"] },
				transferMode: "ReportEvents",
			});
			await evalJs(js);
			await send("Tracing.end");
			await complete.promise;
			listeners.delete(listen);
			return events
				.filter((e) => e.name === "UpdateLayoutTree")
				.reduce((n, e) => n + (e.args?.elementCount ?? 0), 0);
		};

		// Three layers from the real component's markup (current, then two near); the content goes in the body.
		const body = content(nodes);
		const layers = [current, near, near].map((m, i) =>
			m
				.replace(/<!--.*?-->/g, "")
				.replace("@@BODY@@", body)
				.replace('data-space="s"', `data-space="s${i}"`),
		);
		await evalJs(
			`document.getElementById('pager').innerHTML = ${JSON.stringify(layers.join(""))}; document.body.getBoundingClientRect()`,
		);
		const total = (await evalJs(
			`document.querySelectorAll('*').length`,
		)) as number;

		/** What the component changes between its two roles: every attribute that differs, on the root and on the body. */
		const FLIP = `(() => {
			const attrs = (html) => { const d = new DOMParser().parseFromString(html.replace('@@BODY@@', ''), 'text/html');
				const r = d.body.firstElementChild; return [r, r.querySelector('.space-body')].map((e) => Object.fromEntries([...e.attributes].map((a) => [a.name, a.value]))); };
			const cur = attrs(${JSON.stringify(current)}), near = attrs(${JSON.stringify(near)});
			const apply = (l, want) => { const els = [l, l.querySelector('.space-body')];
				els.forEach((e, k) => { for (const n of [...e.getAttributeNames()]) if (!(n in want[k]) && n !== 'class' && n !== 'style' && n !== 'data-space') e.removeAttribute(n);
					for (const [n, v] of Object.entries(want[k])) if (n !== 'class' && n !== 'style' && n !== 'data-space' && e.getAttribute(n) !== v) e.setAttribute(n, v); }); };
			const [a, b] = [...document.querySelectorAll('.space-layer')].slice(0, 2);
			if (a.dataset.role === 'current') { apply(a, near); apply(b, cur); } else { apply(b, near); apply(a, cur); }
			document.body.getBoundingClientRect();
		})()`;
		await evalJs(FLIP); // warm: first update builds the style caches
		await evalJs(FLIP);

		const runs: Cost[] = [];
		for (let k = 0; k < 7; k++) {
			const before = await metric();
			const t0 = (await evalJs(`performance.now()`)) as number;
			const elements = await traced(FLIP);
			const t1 = (await evalJs(`performance.now()`)) as number;
			const after = await metric();
			runs.push({
				recalcMs:
					(after.RecalcStyleDuration - before.RecalcStyleDuration) * 1000,
				recalcs: after.RecalcStyleCount - before.RecalcStyleCount,
				elements,
				wallMs: t1 - t0,
			});
		}
		runs.sort((x, y) => x.recalcMs - y.recalcMs);
		const med = runs[Math.floor(runs.length / 2)];
		const worst = runs[runs.length - 1];
		const report = {
			nodes: total,
			layers: 3,
			budgetMs,
			medianRecalcMs: +med.recalcMs.toFixed(2),
			worstRecalcMs: +worst.recalcMs.toFixed(2),
			elementsRestyled: med.elements,
		};
		if (process.argv.includes("--json")) console.log(JSON.stringify(report));
		else
			console.log(
				`role flip of 2 layers over ${total} nodes: style recalc median ${report.medianRecalcMs} ms, worst ${report.worstRecalcMs} ms, ${report.elementsRestyled} elements restyled (budget ${budgetMs} ms)`,
			);
		// A flip may restyle the layer roots and a few wrappers, never the content.
		const bad: string[] = [];
		if (med.recalcMs > budgetMs)
			bad.push(`style recalc ${med.recalcMs.toFixed(1)} ms > ${budgetMs} ms`);
		if (med.elements > 200)
			bad.push(
				`${med.elements} elements restyled by a role flip (the content must keep its style)`,
			);
		if (bad.length) {
			console.error(
				`role flip restyles the layer contents:\n  ${bad.join("\n  ")}`,
			);
			process.exitCode = 1;
		}
	} finally {
		proc.kill();
		await proc.exited;
		rmSync(dir, { recursive: true, force: true });
	}
}

if (import.meta.main) await main();
