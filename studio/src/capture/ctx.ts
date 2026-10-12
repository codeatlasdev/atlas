// What a scene script can do to the isolated Atlas Desktop instance. Everything goes through the debug bridge
// (`$HOME/.atlas/desktop-debug.sock`, the same JSON-lines protocol as `atlas desktop debug eval`): synthetic DOM
// events inside the webview and calls on the live stores in `window.__atlas`. Nothing here ever sends input to the
// operating system, so the user's own desktop is never touched.

import type { Socket } from "bun";
import { StudioError } from "../lib/paths.ts";

/** Injected once per app instance; the scenes call it through `ctx`. Runs inside the webview. */
const HELPERS = String.raw`
window.__film = {
  key(s) { // "cmd-shift-p", "ctrl-cmd-s", "alt-cmd-right", "escape", "enter"
    const p = s.split("-"); let k = p.pop(); if (s.endsWith("--")) k = "-";
    const named = {escape:"Escape",enter:"Enter",left:"ArrowLeft",right:"ArrowRight",up:"ArrowUp",down:"ArrowDown",tab:"Tab"};
    const key = named[k] ?? k;
    const ev = new KeyboardEvent("keydown", {key, code: /^[a-z]$/.test(k)?"Key"+k.toUpperCase():(/^\d$/.test(k)?"Digit"+k:key), metaKey:p.includes("cmd"), shiftKey:p.includes("shift"), ctrlKey:p.includes("ctrl"), altKey:p.includes("alt"), bubbles:true, cancelable:true});
    (document.activeElement || document.body).dispatchEvent(ev);
    return ev.defaultPrevented;
  },
  click(sel) { const e = typeof sel==="string" ? document.querySelector(sel) : sel; e?.click(); return !!e; },
  byText(txt, tag="button,[role=menuitem],[role=option],a,div,span") { return [...document.querySelectorAll(tag)].find(e => e.textContent.trim() === txt && e.offsetParent !== null); },
  // timer-driven, not requestAnimationFrame: the isolated instance is never frontmost and WebKit throttles rAF for background windows
  smooth(sel, to, ms) { return new Promise(res => { const el = document.querySelector(sel); const from = el.scrollTop; const t0 = performance.now(); const ease = t => t<.5 ? 4*t*t*t : 1-Math.pow(-2*t+2,3)/2; const id = setInterval(() => { const t = Math.min(1,(performance.now()-t0)/ms); el.scrollTop = from + (to-from)*ease(t); if (t >= 1) { clearInterval(id); res(el.scrollTop); } }, 16); }); },
  async typeInto(sel, text, delay=90) { const el = document.querySelector(sel); el.focus(); const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,"value").set; for (const ch of text) { setter.call(el, el.value + ch); el.dispatchEvent(new InputEvent("input",{bubbles:true,data:ch,inputType:"insertText"})); await new Promise(r=>setTimeout(r,delay)); } return el.value; },
  clear(sel) { const el=document.querySelector(sel); Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,"value").set.call(el,""); el.dispatchEvent(new InputEvent("input",{bubbles:true})); },
};
return 1;
`;

/** One reply line of the debug bridge (crates/atlas-desktop/src/debug.rs). */
interface BridgeReply {
	ok: boolean;
	result?: unknown;
	error?: string;
}

/** One request/reply connection to the app's debug bridge. */
class Bridge {
	private socket!: Socket<undefined>;
	private buffer = "";
	private waiting: { promise: Promise<string>; resolve(line: string): void; reject(e: Error): void } | null = null;
	private lines: string[] = [];

	static async open(path: string): Promise<Bridge> {
		const b = new Bridge();
		b.socket = await Bun.connect({
			unix: path,
			socket: {
				data: (_s, chunk) => b.onData(chunk.toString("utf8")),
				close: () => b.waiting?.reject(new StudioError("the debug bridge closed the connection", "is the app still running?")),
			},
		});
		return b;
	}

	private onData(text: string) {
		this.buffer += text;
		for (let nl = this.buffer.indexOf("\n"); nl >= 0; nl = this.buffer.indexOf("\n")) {
			this.lines.push(this.buffer.slice(0, nl));
			this.buffer = this.buffer.slice(nl + 1);
		}
		if (this.waiting && this.lines.length) {
			const w = this.waiting;
			this.waiting = null;
			w.resolve(this.lines.shift()!);
		}
	}

	async request(req: Record<string, unknown>): Promise<any> {
		this.waiting = Promise.withResolvers<string>();
		this.socket.write(`${JSON.stringify(req)}\n`);
		const reply: BridgeReply = JSON.parse(await this.waiting.promise);
		if (!reply.ok) throw new StudioError(`app eval failed: ${reply.error}`, String(req.js ?? req.cmd).slice(0, 300));
		return reply.result;
	}

	close() {
		this.socket.end();
	}
}

export interface Ctx {
	/** Evaluate a JS body (use `return`) in the webview and get its JSON result. 10 s cap per call (bridge limit). */
	eval<T = unknown>(js: string): Promise<T>;
	sleep(ms: number): Promise<void>;
	/** Dispatch a key combo like "ctrl-cmd-s", "alt-cmd-right", "escape" to the focused element. */
	key(combo: string): Promise<void>;
	/** Type into an input matched by `selector`, one character per `delayMs`. */
	typeInto(selector: string, text: string, delayMs?: number): Promise<void>;
	/** Clear the text of an input matched by `selector`. */
	clearInput(selector: string): Promise<void>;
	/** Click the first visible element matching `selector`; false when there is none. */
	click(selector: string): Promise<boolean>;
	/** Click the first visible button/menu item/etc. whose text is exactly `text`; false when there is none. */
	clickText(text: string): Promise<boolean>;
	/** Show the global Agents or Dashboard view, or go back to the worktree. */
	view(view: "agents" | "dashboard" | "worktree"): Promise<void>;
	/** Eased scroll of the element matching `selector` to `top` px over `ms`. */
	smoothScroll(selector: string, top: number, ms: number): Promise<void>;
	/** Show or hide the left sidebar (⌃⌘S toggles it). */
	sidebar(state: "show" | "hide"): Promise<void>;
	/** Switch to the space (project) with this name, animated like a real switch. */
	space(name: string): Promise<void>;
	/** Select the worktree (by project name and worktree name or branch) and show its tab layout. */
	worktree(project: string, name?: string): Promise<string>;
	/** Open (or focus) the Panel / Logs tab of the selected worktree. */
	openTab(kind: "services" | "logs" | "config"): Promise<void>;
	/** Start a terminal in the selected worktree running `argv`; resolves with the tab id once it is the active tab. */
	terminal(argv: string[], opts?: { label?: string; env?: Record<string, string> }): Promise<string>;
	/** Start a plain login-shell terminal in the selected worktree and type `command` into it key by key. */
	shellRun(command: string, typeDelayMs?: number): Promise<void>;
	/** Start the project's stack (like the Start button) and wait until `up` services are running. */
	startStack(project: string, up: number, timeoutMs?: number): Promise<void>;
	/** Scene-specific facts the app instance exposes (scratch paths, fixture ids). */
	env: SceneEnv;
}

export interface SceneEnv {
	/** Absolute scratch project roots by project name (`orbit-shop`, `lumen-app`, …). */
	projects: Record<string, string>;
	/** Isolated HOME of the instance. */
	home: string;
	/** The atlas CLI the instance uses (`ATLAS_BIN`). */
	atlasBin: string;
	/** Directory with the fake `omp`, the demo scripts and the sample image. */
	assets: string;
	fakeAgentDir: string;
}

export interface Scene {
	id: string;
	/** Length of the window recording. */
	seconds: number;
	/** Runs before recording starts (not captured): put the app in the starting state. */
	setup?(ctx: Ctx): Promise<void>;
	/** Runs while the recorder is rolling, for exactly the scene's duration. */
	body(ctx: Ctx): Promise<void>;
}

export const sleep = Bun.sleep;

/** Open the bridge of the instance whose HOME is `home`, install the helpers and return the scene context. */
export async function makeCtx(home: string, env: SceneEnv): Promise<{ ctx: Ctx; close(): void }> {
	const bridge = await Bridge.open(`${home}/.atlas/desktop-debug.sock`);
	const ev = async <T>(js: string): Promise<T> => {
		const result: T = await bridge.request({ cmd: "eval", js }); // the scene states what its JS returns
		return result;
	};
	await ev(HELPERS);

	const wt = (project: string, name?: string) =>
		`(() => { const a = window.__atlas; const repo = a.store.projects.find(p => !p.parent && p.name === ${JSON.stringify(project)}); if (!repo) throw new Error("no project ${project}");
		  return a.ws.doc.worktrees.find(w => w.repoId === repo.id && (${JSON.stringify(name ?? null)} === null ? w.isMain : (w.name === ${JSON.stringify(name)} || w.branch.endsWith(${JSON.stringify(name)})))); })()`;

	const ctx: Ctx = {
		eval: ev,
		sleep,
		async key(combo) {
			await ev(`return window.__film.key(${JSON.stringify(combo)})`);
		},
		async typeInto(selector, text, delayMs = 90) {
			await ev(`return await window.__film.typeInto(${JSON.stringify(selector)}, ${JSON.stringify(text)}, ${delayMs})`);
		},
		async clearInput(selector) {
			await ev(`window.__film.clear(${JSON.stringify(selector)}); return 1`);
		},
		click: (selector) => ev<boolean>(`return window.__film.click(${JSON.stringify(selector)})`),
		clickText: (text) => ev<boolean>(`return window.__film.click(window.__film.byText(${JSON.stringify(text)}) ?? null)`),
		async view(view) {
			await ev(`window.__atlas.ws.view = ${JSON.stringify(view)}; return 1`);
			await sleep(400);
		},
		async smoothScroll(selector, top, ms) {
			await ev(`return await window.__film.smooth(${JSON.stringify(selector)}, ${top}, ${ms})`);
		},
		async sidebar(state) {
			const hidden = await ev<boolean>(`return document.querySelector(".sidebar-slot").classList.contains("hidden")`);
			if ((state === "show") === hidden) await ctx.key("ctrl-cmd-s");
			await sleep(400);
		},
		async space(name) {
			const hit = await ev<boolean>(`const b = document.querySelector('.spaces .space[aria-label=${JSON.stringify(name)}]'); b?.click(); return !!b`);
			if (!hit) throw new StudioError(`no space button for "${name}"`, "the sidebar space strip needs 2+ projects");
			await sleep(500);
		},
		async worktree(project, name) {
			const id = await ev<string>(`const a = window.__atlas; const w = ${wt(project, name)}; if (!w) throw new Error("no worktree ${project}/${name ?? "main"}");
				a.ws.view = "worktree"; a.store.selected = w.id; a.ws.select(w.id); a.ws.ensureLayout(w.id); a.ws.ensurePanel(w.id); return w.id`);
			await sleep(400);
			return id;
		},
		async openTab(kind) {
			const labels = { services: "Panel", logs: "Logs", config: "Config" };
			await ev(`const a = window.__atlas; const w = a.ws.selected; a.ws.reveal(w.id, t => t.kind === ${JSON.stringify(kind)}, { kind: ${JSON.stringify(kind)}, label: ${JSON.stringify(labels[kind])} }); return 1`);
			await sleep(500);
		},
		terminal: (argv, opts) =>
			ev<string>(`const a = window.__atlas; const w = a.ws.selected;
				const t = await a.ws.newTerminal(w.id, { argv: ${JSON.stringify(argv)}, label: ${JSON.stringify(opts?.label ?? "Terminal")}, env: ${JSON.stringify(opts?.env ?? {})} });
				if (!t) throw new Error("terminal did not start"); return t.id`),
		async shellRun(command, typeDelayMs = 45) {
			const id = await ctx.terminal([]);
			await sleep(1500);
			const pty = await ev<string>(`return window.__atlas.ws.tab(${JSON.stringify(id)}).pty`);
			for (const ch of `${command}\r`) {
				await ev(`await window.__atlas.host.ptyWrite(${JSON.stringify(pty)}, ${JSON.stringify(ch)}); return 1`);
				await sleep(typeDelayMs);
			}
		},
		async startStack(project, up, timeoutMs = 60_000) {
			await ev(`const a = window.__atlas; const p = a.store.projects.find(p => !p.parent && p.name === ${JSON.stringify(project)}); window.__TAURI_INTERNALS__.invoke("stack_start", { id: p.id, services: [], profiles: [] }).catch(e => console.error("stack_start failed", e)); return 1`); // not awaited: it can outlast the bridge's 10 s eval cap
			const deadline = Date.now() + timeoutMs;
			for (;;) {
				const n = await ev<number>(`const a = window.__atlas; const p = a.store.projects.find(p => !p.parent && p.name === ${JSON.stringify(project)}); return (a.store.live[p.id]?.services ?? []).filter(s => s.state === "running").length`);
				if (n >= up) return;
				if (Date.now() > deadline) throw new StudioError(`${project}: only ${n}/${up} services running after ${timeoutMs / 1000}s`, "check the scratch stack logs; ports 4100/4101 free?");
				await sleep(500);
			}
		},
		env,
	};
	return { ctx, close: () => bridge.close() };
}
