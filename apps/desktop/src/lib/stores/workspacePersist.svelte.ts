/** Workspace persistence: load the saved doc, autosave it (debounced), follow hostd events. */

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { toast } from "@codeatlasdev/ui";
import { errText } from "#lib/api.ts";
import { errors } from "#lib/errors/errors.svelte.ts";
import { hostTextLoaded, parseWire } from "#lib/hostText.ts";
import { m } from "#lib/i18n/m.ts";
import { docShapeError, emptyDoc, type Doc } from "#lib/stores/workspaceTypes.ts";
import { docSerializer } from "#lib/stores/workspaceSnapshot.ts";

const SAVE_DEBOUNCE_MS = 300;
/** The notice stays long enough to read a path. */
const NOTICE_MS = 15_000;

/** `workspace::Recovered` (crates/atlas-desktop/src/workspace.rs): a workspace.json that could not be used. `kept` is
 * where its bytes are now; null means it is still at `path` and must not be saved over. */
interface Recovered {
	path: string;
	kept: string | null;
	error: string | null;
}

/** `workspace::WorkspaceLoad`. */
interface WorkspaceLoad {
	doc: Doc | null;
	recovered: Recovered | null;
}

/** What persistence needs from the store (its reactive doc and host-sync state). */
interface Target {
	doc: Doc;
	loaded: boolean;
	ptyEvent(e: { ev: string; pty?: string; title?: string; code?: number | null }): void;
	hostError: string | null;
	syncPtys(): Promise<void>;
}

export class WorkspacePersist {
	/** The autosave is running. False after a load that left an unusable file in place: this session never writes over it. */
	saving = false;
	#saveTimer: number | undefined;
	#unlisten: UnlistenFn[] = [];

	constructor(private ws: Target) {}

	async init() {
		const ws = this.ws;
		// The saved doc and the listeners are independent IPCs: wait for them together. Each
		// handle is kept as it resolves, so `dispose()` sees it even if the load fails.
		const track = (p: Promise<UnlistenFn>) => p.then((u) => void this.#unlisten.push(u));
		// Bumped by every reconnect: an error still waiting for its messages when that happens is stale and dropped.
		let reconnects = 0;
		const [{ doc: raw, recovered }] = await Promise.all([
			this.#read(),
			track(
				listen<{ ev: string; pty?: string; title?: string; code?: number | null }>("host://event", (e) =>
					ws.ptyEvent(e.payload),
				),
			),
			track(
				// A payload is `{code, args, message}` (or a bare string from an older shell).
				listen<unknown>("host://error", async (e) => {
					const seen = reconnects;
					const text = await hostTextLoaded(parseWire(e.payload) ?? {});
					if (seen !== reconnects) return;
					ws.hostError = text;
					// The connect loop retries every few seconds: one toast with a count, not one per retry.
					void errors.captureHost(text);
				}),
			),
			track(
				listen("host://reconnected", () => {
					reconnects++;
					ws.hostError = null;
					errors.hostReachable();
					void ws.syncPtys();
				}),
			),
		]);
		ws.doc = { ...emptyDoc(), ...(raw ?? {}) };
		ws.loaded = true;
		if (recovered)
			toast.error(
				recovered.kept
					? m.shell_toasts_workspace_recovered({ path: recovered.kept })
					: m.shell_toasts_workspace_unreadable({ reason: recovered.error ?? "", path: recovered.path }),
				NOTICE_MS,
			);
		await ws.syncPtys();
		// The file is still where it was: saving this session's empty workspace would be the data loss the notice is about.
		if (recovered && !recovered.kept) return;
		const serialize = docSerializer();
		$effect.root(() => {
			$effect(() => {
				const snapshot = serialize(ws.doc);
				clearTimeout(this.#saveTimer);
				this.#saveTimer = window.setTimeout(() => {
					invoke("workspace_save", { doc: JSON.parse(snapshot) }).catch((e) => toast.error(errText(e)));
				}, SAVE_DEBOUNCE_MS);
			});
		});
		this.saving = true;
	}

	/** The saved document, always: a file that cannot be used (not JSON, not a workspace, not readable) opens an empty
	 * workspace and is reported, never a rejected load, which left `ws.loaded` false and the ready gate shut for good. */
	async #read(): Promise<{ doc: Doc | null; recovered: Recovered | null }> {
		let res: WorkspaceLoad;
		try {
			res = await invoke<WorkspaceLoad>("workspace_load");
		} catch (e) {
			return { doc: null, recovered: { path: "workspace.json", kept: null, error: errText(e) } };
		}
		const why = res.recovered ? null : docShapeError(res.doc);
		if (!why) return res;
		// Valid JSON, not a workspace: the schema is ours, so the move aside is asked for here.
		try {
			return { doc: null, recovered: await invoke<Recovered>("workspace_set_aside") };
		} catch (e) {
			return { doc: null, recovered: { path: "workspace.json", kept: null, error: `${why}; ${errText(e)}` } };
		}
	}

	dispose() {
		this.#unlisten.forEach((u) => u());
		this.#unlisten = [];
	}
}
