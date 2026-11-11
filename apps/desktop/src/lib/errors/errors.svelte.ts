/**
 * Non-render errors (event handlers, timers, promises) never reach a `<svelte:boundary>`.
 * `startErrorHandler` funnels window `error` / `unhandledrejection` into one place: log it
 * (`console.error`, which the debug bridge captures), and show one non-blocking toast per
 * distinct error with a "Details" action that opens the same panel used for crashed views.
 */
import { invoke } from "@tauri-apps/api/core";
import { crumbs } from "./breadcrumbs.ts";
import { gatherContext } from "./context.ts";
import { classify, hostErrorReport, parseDiagnostics } from "./diagnostics.ts";
import { fingerprint, isBenign, messageOf, report, withContext, type ErrorReport } from "./report.ts";

/** Same error inside this window collapses into one toast. */
const DEDUPE_MS = 10_000;
const MAX_SEEN = 50;
/** An outage keeps producing the same failure for as long as it lasts: one toast per minute, not per call. */
const HOST_DEDUPE_MS = 60_000;

/** Host failures read the same everywhere: strip the home directory the OS error carried. */
const hostText = (m: string) => m.replace(/\/(?:Users|home)\/[^/\s)]+/g, "~").trim();

class Errors {
	/** The error whose details dialog is open. */
	current = $state<ErrorReport | null>(null);
	/** Toasts waiting to be rendered by `ErrorToasts`; the store owns dedupe, the view owns layout. */
	toasts = $state<{ id: number; report: ErrorReport; repeats: number; host?: boolean }[]>([]);
	/** Debug builds only (`window.__atlas.crashTab`): ids whose next render throws, to exercise the boundaries. */
	crashing = $state<Record<string, true>>({});
	#version: Promise<string | undefined> | undefined;
	#seen = new Map<string, { at: number; id: number }>();
	#next = 1;

	/** Cached `system_info().app.version`; undefined when the call fails (tray webview, tests). */
	version(): Promise<string | undefined> {
		this.#version ??= invoke<{ app?: { version?: string } } | null>("system_info").then(
			(i) => i?.app?.version,
			() => undefined,
		);
		return this.#version;
	}

	/** Add machine facts and the recent breadcrumbs; never throws. */
	async enrich(r: ErrorReport): Promise<ErrorReport> {
		const ctx = await gatherContext({
			invoke: (cmd) => invoke(cmd),
			nav: navigator,
			route: location.pathname,
			crumbs: crumbs.snapshot(),
		});
		return withContext(r, ctx);
	}

	async capture(e: unknown, where: string) {
		const message = messageOf(e);
		if (isBenign(e, message)) return;
		if (classify(message) === "host") return this.captureHost(message);
		const r = await this.enrich({ ...report(e, { where }), version: await this.version() });
		crumbs.add(`error: ${r.message.split("\n")[0]}`);
		console.error(`[atlas] ${where}: ${r.message}`, e instanceof Error ? e : "");
		const fp = fingerprint(r);
		const now = Date.now();
		const prev = this.#seen.get(fp);
		if (prev && now - prev.at < DEDUPE_MS) {
			prev.at = now;
			const t = this.toasts.find((x) => x.id === prev.id);
			if (t) t.repeats += 1;
			return;
		}
		if (this.#seen.size >= MAX_SEEN) this.#seen.delete(this.#seen.keys().next().value as string);
		// Installed builds have no devtools: the app log is the only place a page error survives.
		invoke("diag_frontend_error", { place: where, message: r.message }).catch(() => {});
		const id = this.#next++;
		this.#seen.set(fp, { at: now, id });
		this.toasts.push({ id, report: r, repeats: 0 });
	}

	/**
	 * A failure of the terminal host (`host_call`, `host_pty_list`, `host://error`). Every tab, poll and
	 * restart that hits the same outage reports it, so they fold into ONE toast with a count (1.5.0-alpha.5
	 * showed three identical ones). The key drops the per-call detail in parentheses (paths, errnos).
	 */
	captureHost(message: string) {
		const text = hostText(message);
		const key = `host:${text.replace(/\s*\([^)]*\)\s*$/, "")}`;
		const now = Date.now();
		const prev = this.#seen.get(key);
		const live = prev && this.toasts.find((x) => x.id === prev.id);
		if (prev && live && now - prev.at < HOST_DEDUPE_MS) {
			prev.at = now;
			live.repeats += 1;
			return;
		}
		const id = this.#next++;
		this.#seen.set(key, { at: now, id });
		crumbs.add(`host: ${text.split("\n")[0]}`);
		// Pushed before any await: concurrent callers (three tabs, a poll) must see it and fold into it.
		const t = {
			id,
			repeats: 0,
			host: true,
			report: {
				message: text,
				stack: "",
				where: "terminal host",
				kind: "hostd",
				version: undefined as string | undefined,
			},
		};
		this.toasts.push(t);
		void this.version().then((v) => {
			const live = this.toasts.find((x) => x.id === id);
			if (live) live.report.version = v;
		});
	}

	/** Versions, OS and the redacted log tails; best effort, the screen is already showing a failure. */
	async diagnose(r: ErrorReport): Promise<ErrorReport> {
		const raw = await invoke("host_diagnostics").catch(() => null);
		const d = parseDiagnostics(raw);
		return {
			...(await this.enrich(r)),
			...hostErrorReport(r.message, d),
			context: { ...(await this.enrich(r)).context, breadcrumbs: crumbs.snapshot(), diagnostics: d },
		};
	}

	/** "Restart terminal host": ends its terminals; works when the host answers and when it does not. */
	async restartHost(toastId?: number) {
		if (toastId !== undefined) this.dismiss(toastId);
		await invoke("host_restart_force").catch((e) => this.capture(e, "restart terminal host"));
	}

	/** Open the details dialog with the diagnostics attached (Copy as Markdown / Report read them). */
	async showHost(t: { id: number; report: ErrorReport }) {
		this.show(await this.diagnose(t.report), t.id);
	}

	dismiss(id: number) {
		this.toasts = this.toasts.filter((t) => t.id !== id);
	}

	show(r: ErrorReport, toastId?: number) {
		this.current = r;
		if (toastId !== undefined) this.dismiss(toastId);
	}
}

export const errors = new Errors();

/** Install the window listeners. Returns the cleanup. */
export function startErrorHandler(where = "window"): () => void {
	const onError = (ev: ErrorEvent) => void errors.capture(ev.error ?? ev.message, where);
	const onRejection = (ev: PromiseRejectionEvent) => void errors.capture(ev.reason, where);
	window.addEventListener("error", onError);
	window.addEventListener("unhandledrejection", onRejection);
	return () => {
		window.removeEventListener("error", onError);
		window.removeEventListener("unhandledrejection", onRejection);
	};
}
