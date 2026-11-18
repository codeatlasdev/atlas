/** Workspace persistence: load the saved doc, autosave it (debounced), follow hostd events. */

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { toast } from "@codeatlasdev/ui";
import { errText } from "#lib/api.ts";
import { errors } from "#lib/errors/errors.svelte.ts";
import { hostText, parseWire } from "#lib/hostText.ts";
import { emptyDoc, type Doc } from "#lib/stores/workspaceTypes.ts";
import { tabTitle } from "#lib/title.ts";

const SAVE_DEBOUNCE_MS = 300;

/** What persistence needs from the store (its reactive doc and host-sync state). */
interface Target {
	doc: Doc;
	loaded: boolean;
	titles: Record<string, string>;
	exited: Record<string, number | null>;
	hostError: string | null;
	syncPtys(): Promise<void>;
}

export class WorkspacePersist {
	#saveTimer: number | undefined;
	#unlisten: UnlistenFn[] = [];

	constructor(private ws: Target) {}

	async init() {
		const ws = this.ws;
		// The saved doc and the listeners are independent IPCs: wait for them together. Each
		// handle is kept as it resolves, so `dispose()` sees it even if the load fails.
		const track = (p: Promise<UnlistenFn>) => p.then((u) => void this.#unlisten.push(u));
		const [raw] = await Promise.all([
			invoke<Doc | null>("workspace_load"),
			track(
				listen<{ ev: string; pty?: string; title?: string; code?: number | null }>("host://event", (e) => {
					const p = e.payload;
					if (p.ev === "pty_title" && p.pty) ws.titles[p.pty] = tabTitle(p.title ?? "");
					if (p.ev === "pty_exit" && p.pty) ws.exited[p.pty] = p.code ?? null;
				}),
			),
			track(
				// A payload is `{code, args, message}` (or a bare string from an older shell).
				listen<unknown>("host://error", (e) => {
					const text = hostText(parseWire(e.payload) ?? {});
					ws.hostError = text;
					// The connect loop retries every few seconds: one toast with a count, not one per retry.
					void errors.captureHost(text);
				}),
			),
			track(
				listen("host://reconnected", () => {
					ws.hostError = null;
					errors.hostReachable();
					void ws.syncPtys();
				}),
			),
		]);
		ws.doc = { ...emptyDoc(), ...(raw ?? {}) };
		ws.loaded = true;
		await ws.syncPtys();
		$effect.root(() => {
			$effect(() => {
				const snapshot = JSON.stringify(ws.doc);
				clearTimeout(this.#saveTimer);
				this.#saveTimer = window.setTimeout(() => {
					invoke("workspace_save", { doc: JSON.parse(snapshot) }).catch((e) => toast.error(errText(e)));
				}, SAVE_DEBOUNCE_MS);
			});
		});
	}

	dispose() {
		this.#unlisten.forEach((u) => u());
		this.#unlisten = [];
	}
}
