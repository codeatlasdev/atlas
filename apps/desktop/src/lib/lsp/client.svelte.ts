/**
 * Frontend LSP client over hostd (`lsp_*` commands, `lsp_*` pushes; see crates/atlas-host protocol).
 * One listener for the whole app; each open editor registers a `LspDoc` that owns its document sync:
 * `lsp_open`, debounced + coalesced `lsp_change` (UTF-16 incremental events), `lsp_save`, `lsp_close`,
 * and a re-open of every live document when hostd reconnects (a new daemon has no document state).
 */
import { listen } from "@tauri-apps/api/event";
import { untrack } from "svelte";
import type { ChangeSet, Text } from "@codemirror/state";
import { hostCall } from "#lib/host.ts";
import { ChangeBuffer, pathToUri, type TextEdit, type WorkspaceEdit } from "#lib/lsp/convert.ts";
import { mergeLspOverrides } from "#lib/lsp/overrides.ts";
import type { ProjectLsp } from "#lib/lsp/projectParse.ts";
import { invalidateProjectSettings, isProjectSettingsPath, projectLsp } from "#lib/lsp/projectSettings.ts";
import type { Catalog, LspOverride, Reason, ServerStatus } from "#lib/lsp/types.ts";
import { isHostTooOld, openPayload } from "#lib/lsp/hostFacts.ts";
import { startSync } from "#lib/lsp/sync.ts";
import { prefs } from "#lib/stores/prefs.svelte.ts";
import { m } from "#lib/i18n/m.ts";

const CHANGE_DEBOUNCE_MS = 40;
const PULL_DEBOUNCE_MS = 400;

export interface LspDiagnostic {
	range: { start: { line: number; character: number }; end: { line: number; character: number } };
	severity?: 1 | 2 | 3 | 4;
	code?: string | number;
	source?: string;
	message: string;
	tags?: number[];
	data?: unknown;
}

export type {
	Catalog,
	CatalogEntry,
	LspOverride,
	OpenStatus,
	Reason,
	ReasonCode,
	ServerSource,
	ServerState,
	ServerStatus,
} from "#lib/lsp/types.ts";

/** Subset of `ServerCapabilities` the editor gates features on. */
export interface Capabilities {
	completionProvider?: { triggerCharacters?: string[]; resolveProvider?: boolean };
	diagnosticProvider?: unknown;
	signatureHelpProvider?: { triggerCharacters?: string[]; retriggerCharacters?: string[] };
	hoverProvider?: unknown;
	definitionProvider?: unknown;
	referencesProvider?: unknown;
	renameProvider?: boolean | { prepareProvider?: boolean };
	documentFormattingProvider?: unknown;
	codeActionProvider?: unknown;
	documentHighlightProvider?: unknown;
	inlayHintProvider?: unknown;
	documentSymbolProvider?: unknown;
}

interface OpenReply {
	server: string | null;
	status: string;
	capabilities?: Capabilities | null;
	reason?: Reason;
	supported?: string[];
}

/** Reactive state the UI reads: server list and per-path diagnostics. */
export const lsp = $state<{
	servers: ServerStatus[];
	diagnostics: Record<string, LspDiagnostic[]>;
}>({ servers: [], diagnostics: {} });

type DiagListener = (version: number | null | undefined, diags: LspDiagnostic[]) => void;
type EditHandler = (edit: WorkspaceEdit) => Promise<boolean>;

const docs: Record<string, LspDoc> = {};
let editHandler: EditHandler | null = null;
let rid = 0;
let stopSync: (() => void) | null = null;

/** Registered by the workspace: applies a server-initiated WorkspaceEdit (open docs + disk). */
export const setApplyEditHandler = (h: EditHandler) => (editHandler = h);

/** Start the app-wide hostd sync (idempotent): the server list at start, status pushes, diagnostics, reconnects. */
export function startLspEvents() {
	stopSync ??= startSync({
		listen: (ev, h) => listen(ev, h),
		fetchServers: async () => (await hostCall<{ servers: ServerStatus[] }>("lsp_status")).servers,
		apply: (s) => applyServers(s as ServerStatus[]),
		reopenAll,
		onEvent: (p) => onEvent(p as Record<string, unknown> & { ev: string }),
	});
}

function onEvent(p: Record<string, unknown> & { ev: string }) {
	if (p.ev === "lsp_status") applyServers(p.servers as ServerStatus[]);
	else if (p.ev === "lsp_diagnostics") {
		const path = p.path as string;
		const diags = p.diagnostics as LspDiagnostic[];
		lsp.diagnostics[path] = diags;
		docs[path]?.onDiagnostics(p.version as number | null | undefined, diags);
	} else if (p.ev === "lsp_apply_edit") {
		const id = String(p.id);
		const run = editHandler?.(p.edit as WorkspaceEdit) ?? Promise.resolve(false);
		void run
			.catch(() => false)
			.then((applied) => hostCall("lsp_apply_edit_result", { id, applied }).catch(() => {}));
	}
}

/** Replace the server list; documents that were waiting on a server that is now ready re-open. */
function applyServers(next: ServerStatus[]) {
	const was = lsp.servers;
	lsp.servers = next;
	for (const d of Object.values(docs)) {
		if (d.open || !d.replied) continue;
		const before = was.find((s) => s.name === d.server && s.state === "ready");
		const now = next.find((s) => s.name === d.server && s.state === "ready");
		if (now && !before) void d.reopen();
	}
}

/** Refresh the server list (the push keeps it current afterwards). */
export async function refreshStatus() {
	applyServers((await hostCall<{ servers: ServerStatus[] }>("lsp_status")).servers);
}

/** Ask hostd again for every open document (after a toolchain was installed or a setting changed). */
export function reopenAll() {
	for (const d of Object.values(docs)) void d.reopen();
}

/** The server (if any) handling `path`: the one whose root contains it, deepest root first. */
export function serverFor(path: string): ServerStatus | undefined {
	return lsp.servers
		.filter((s) => path.startsWith(s.root.endsWith("/") ? s.root : `${s.root}/`))
		.sort((a, b) => b.root.length - a.root.length)[0];
}

/** Restart the server `name` for `root` (clears a crash; the status push follows). */
export async function restartServer(name: string, root?: string) {
	await hostCall("lsp_restart", { name, ...(root ? { root } : {}) });
	await refreshStatus().catch(() => {});
}

/** Download and install the managed server `name`; progress arrives as `downloading` status pushes. */
export async function installServer(name: string, reinstall = false) {
	await hostCall("lsp_install", { name, reinstall });
	await refreshStatus().catch(() => {});
}

/** Stop an install in progress (uninstalls the partial download). */
export async function cancelInstall(name: string) {
	await hostCall("lsp_uninstall", { name });
	await refreshStatus().catch(() => {});
}

/** Installed servers, what was detected on this machine, and the supported language names. */
export const catalog = () => hostCall<Catalog>("lsp_catalog");

/** Log file of server `name`: its absolute path and the last `lines` lines. */
export const serverLog = (name: string, lines = 200) =>
	hostCall<{ path: string; text: string }>("lsp_log", { name, lines });

export class LspDoc {
	version = 1;
	/** Capabilities of the server for this document; `null` until it reports them. */
	caps = $state<Capabilities | null>(null);
	server = $state<string | null>(null);
	status = $state<string>("starting");
	/** Why hostd could not serve the file (missing toolchain, custom command not found…). */
	reason = $state<Reason | undefined>();
	/** Human names of the supported languages, when hostd has no server for this one. */
	supported = $state<string[] | undefined>();
	/** hostd rejected `lsp_open` as an unknown command: it is older than the app (before its caps were read). */
	hostOld = $state(false);
	/** hostd has answered `lsp_open` at least once (before that the state is "starting"). */
	replied = $state(false);
	/** `.atlas/settings.json` is unusable: one line for the status popover. */
	projectWarning = $state<string | undefined>();
	#buf = new ChangeBuffer();
	#timer: number | undefined;
	#open = false;
	#closed = false;
	#diagListener: DiagListener | null = null;
	#pullTimer: number | undefined;
	#text: () => string;
	#project: () => Promise<ProjectLsp>;
	#gate: () => boolean;
	#stopWatch: (() => void) | undefined;
	#root: string | undefined;

	/**
	 * `root` is the worktree root (for `.atlas/settings.json`; without it only user settings apply).
	 * `gated` says whether language servers are off right now (setting, per-language override, or a
	 * host without `lsp`): the document then stays dormant and sends nothing to hostd.
	 */
	constructor(
		readonly path: string,
		readonly languageId: string,
		text: () => string,
		root?: string,
		gated: () => boolean = () => false,
	) {
		this.#text = text;
		this.#gate = gated;
		this.#root = root;
		this.#project = () => (root ? projectLsp(root) : Promise.resolve({ lsp: {} }));
		startLspEvents();
		docs[path] = this;
		void this.reopen();
		// Re-ask hostd when what it would be told changes: the gate (setting, override, host cap) or the
		// language's override itself (command, args, options). The first run only records the baseline.
		let first = true;
		this.#stopWatch = $effect.root(() => {
			$effect(() => {
				void this.#gate();
				void JSON.stringify(prefs.value.lsp?.[languageId] ?? null);
				if (first) {
					first = false;
					return;
				}
				untrack(() => void this.reopen());
			});
		});
	}

	onDiagnostics(version: number | null | undefined, diags: LspDiagnostic[]) {
		// A server with pull diagnostics answers with syntax and semantic errors together; its pushes
		// (syntax only) would replace that list with a shorter one and flicker.
		if (this.caps?.diagnosticProvider) return;
		this.#diagListener?.(version, diags);
	}

	/** Called with every diagnostics push for this path. */
	listenDiagnostics(l: DiagListener) {
		this.#diagListener = l;
	}

	/**
	 * (Re)send `lsp_open` with the current text: first open and after a hostd reconnect. hostd answers
	 * once the server is up (a first open may download it), so edits made meanwhile are not in the text
	 * it was given: they are replayed as one full-document change when the reply lands.
	 */
	async reopen() {
		if (this.#closed) return;
		if (this.#gate()) {
			// Settings or the host say no language server: nothing to ask hostd. The status item explains.
			if (this.#open) void hostCall("lsp_close", { path: this.path }).catch(() => {});
			this.#open = false;
			this.server = null;
			this.status = "starting";
			this.caps = null;
			this.replied = false;
			return;
		}
		this.#buf.clear();
		clearTimeout(this.#timer);
		const sentVersion = this.version;
		const sentText = this.#text();
		try {
			const overrides = await this.#overrides();
			const r = await hostCall<OpenReply>(
				"lsp_open",
				openPayload({
					path: this.path,
					languageId: this.languageId,
					text: sentText,
					version: sentVersion,
					root: this.#root,
					overrides,
				}),
			);
			if (this.#closed) {
				if (r.server) void hostCall("lsp_close", { path: this.path }).catch(() => {});
				return;
			}
			this.hostOld = false;
			this.server = r.server;
			this.status = r.status;
			this.reason = r.reason;
			this.supported = r.supported;
			this.replied = true;
			this.#open = r.server !== null && (r.status === "ready" || r.status === "starting");
			this.caps = r.capabilities ?? null;
			if (this.#open && this.version !== sentVersion) {
				const cur = this.#text();
				if (cur !== sentText) {
					await hostCall("lsp_close", { path: this.path }).catch(() => {});
					await hostCall(
						"lsp_open",
						openPayload({
							path: this.path,
							languageId: this.languageId,
							text: cur,
							version: this.version,
							root: this.#root,
							overrides,
						}),
					).catch(() => {});
				}
			}
			if (this.#open && !this.caps) void this.refreshCaps();
			else this.pullSoon(0);
		} catch (e) {
			this.hostOld = isHostTooOld(e);
			this.status = "missing";
			this.reason = {
				code: "crashed",
				message: m.workbench_lsp_host_unreachable(),
				hint: m.workbench_lsp_host_unreachable_hint(),
			};
			this.replied = true;
			this.#open = false;
			console.warn("lsp_open failed", e);
		}
	}

	/** Overrides for this document's language: user settings, then the project file over them. */
	async #overrides(): Promise<LspOverride | undefined> {
		const project = await this.#project().catch((): ProjectLsp => ({ lsp: {} }));
		this.projectWarning = project.warning;
		return mergeLspOverrides(prefs.value.lsp, project.lsp, this.languageId);
	}

	/** Server capabilities are only known once it is ready; asks again after each status change. */
	async refreshCaps() {
		const c = await this.request<Capabilities | null>("$/capabilities", {}).catch(() => null);
		if (c) this.caps = c;
		this.pullSoon(0);
	}

	/** Pull diagnostics (LSP 3.17) a moment after the last change; one request in flight, latest wins. */
	pullSoon(ms = PULL_DEBOUNCE_MS) {
		if (!this.#open || !this.caps?.diagnosticProvider) return;
		clearTimeout(this.#pullTimer);
		this.#pullTimer = window.setTimeout(() => void this.pull(), ms);
	}

	async pull() {
		if (!this.#open || !this.caps?.diagnosticProvider) return;
		const version = this.version;
		const res = await this.request<{ kind: string; items?: LspDiagnostic[] } | null>(
			"textDocument/diagnostic",
			{ textDocument: { uri: pathToUri(this.path) } },
		).catch(() => null);
		// `unchanged` keeps the previous list; a reply for an older text is stale.
		if (!res || res.kind !== "full" || this.#closed || version !== this.version) return;
		const items = res.items ?? [];
		lsp.diagnostics[this.path] = items;
		this.#diagListener?.(version, items);
	}

	/** Queue an editor change; flushed after a short idle so a burst of keystrokes is one message. */
	change(before: Text, changes: ChangeSet) {
		this.version++;
		if (!this.#open) return;
		this.#buf.push(before, changes);
		clearTimeout(this.#timer);
		this.#timer = window.setTimeout(() => void this.flush(), CHANGE_DEBOUNCE_MS);
		this.pullSoon();
	}

	/** Send pending changes now (called before any request so the server never answers stale text). */
	async flush() {
		clearTimeout(this.#timer);
		if (!this.#open || !this.#buf.pending) return;
		const changes = this.#buf.take();
		if (changes.length === 0) return;
		await hostCall("lsp_change", { path: this.path, version: this.version, changes }).catch(() => {});
	}

	/** Request with automatic flush; the id lets callers cancel (completion superseded by typing). */
	async request<T>(method: string, params: unknown, id?: string): Promise<T | null> {
		if (!this.#open) return null;
		await this.flush();
		return hostCall<T | null>("lsp_request", { path: this.path, method, params, rid: id ?? `r${++rid}` });
	}

	cancel(id: string) {
		void hostCall("lsp_cancel", { id }).catch(() => {});
	}

	async save() {
		// Saving the project settings file changes what hostd should be told for every document of that
		// worktree: drop the cached copy and re-open them so the new overrides apply at once.
		if (this.#root && isProjectSettingsPath(this.#root, this.path)) {
			invalidateProjectSettings(this.#root);
			reopenAll();
		}
		await this.flush();
		if (this.#open) await hostCall("lsp_save", { path: this.path, text: this.#text() }).catch(() => {});
		this.pullSoon(0);
	}

	get open() {
		return this.#open;
	}

	close() {
		this.#stopWatch?.();
		this.#closed = true;
		clearTimeout(this.#timer);
		clearTimeout(this.#pullTimer);
		if (docs[this.path] === this) delete docs[this.path];
		delete lsp.diagnostics[this.path];
		if (this.#open) void hostCall("lsp_close", { path: this.path }).catch(() => {});
		this.#open = false;
	}
}

/** Document of `path` if an editor has it open (rename/code actions need it). */
export const docOf = (path: string): LspDoc | undefined => docs[path];

export type { TextEdit };
