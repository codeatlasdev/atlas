/**
 * What the editor tells the user about language features for one file. A pure reducer from the
 * facts (settings, host, the document's open reply, the live server status) to one state with a
 * chip label, a sentence, optional detail and the actions that fix it. Rendering and IPC live
 * elsewhere (`LspStatus.svelte`, `client.svelte.ts`).
 */
import { lspReasonText } from "#lib/lsp/reasonText.ts";
import type { OpenStatus, Reason, ServerStatus } from "#lib/lsp/types.ts";

export type Tone = "ok" | "busy" | "warn" | "error" | "idle";

export type ActionId =
	| "restart-host"
	| "restart-host-later"
	| "enable-setting"
	| "open-settings-languages"
	| "docs"
	| "install"
	| "cancel-install"
	| "retry"
	| "restart-server"
	| "open-log"
	| "copy-command";

export interface Action {
	id: ActionId;
	label: string;
	/** The button to emphasise. */
	primary?: boolean;
	/** `copy-command`: the text that gets copied. */
	payload?: string;
}

export interface HostFacts {
	/** The connected hostd serves `lsp_*`. */
	lsp: boolean;
	/** The host is upgrading itself live; no terminal is lost. */
	updating?: boolean;
	/** Live terminals a host restart would end. */
	ptys?: { title: string; cwd: string }[];
}

export interface DocFacts {
	status: OpenStatus | string;
	server: string | null;
	reason?: Reason;
	supported?: string[];
}

export interface UiInput {
	prefsEnabled: boolean;
	languageId: string;
	host: HostFacts;
	doc: DocFacts | undefined;
	srv: ServerStatus | undefined;
	outsideRoot?: boolean;
	/** A language-level `disabled: true` override (settings or project file). */
	languageDisabled?: boolean;
}

interface Base {
	label: string;
	tone: Tone;
	summary: string;
	detail?: string;
	/** Last stderr lines (crashed). */
	tail?: string[];
	actions: Action[];
}

export type LspUiState = Base &
	(
		| { kind: "host-old"; ptys: { title: string; cwd: string }[] }
		| { kind: "host-updating" }
		| { kind: "disabled-setting" }
		| { kind: "disabled-language" }
		| { kind: "no-language"; supported: string[] }
		| { kind: "installing"; server: string; progress: string }
		| { kind: "starting"; server: string }
		| { kind: "ready"; server: string; version?: string; source?: string }
		| { kind: "crashed"; server: string }
		| { kind: "missing-toolchain"; server: string; command?: string }
		| { kind: "offline"; server: string }
		| { kind: "install-failed"; server: string }
		| { kind: "custom-not-found"; server: string }
		| { kind: "outside-root"; server: string }
		| { kind: "missing"; server: string }
	);

export type UiKind = LspUiState["kind"];

const SETTINGS: Action = { id: "open-settings-languages", label: "Language settings" };
const LOG: Action = { id: "open-log", label: "Open log" };
const DOCS: Action = { id: "docs", label: "How language servers work" };

/** `typescript` → `TypeScript`-ish fallback when no server label is known. */
const pretty = (name: string) => (name ? name : "language server");

/** Install progress: the server sends free text (`Downloading… 42%`); keep it as is, trimmed. */
const progressText = (srv: ServerStatus | undefined) => (srv?.progress || srv?.message || "").trim();

const copy = (command: string): Action => ({ id: "copy-command", label: "Copy command", payload: command });

function fromReason(server: string, label: string, reason: Reason): LspUiState {
	const detail = reason.hint;
	switch (reason.code) {
		case "no_go":
		case "no_curl": {
			const actions: Action[] = [];
			if (reason.command) actions.push(copy(reason.command));
			actions.push({ id: "retry", label: "Check again", primary: !reason.command }, SETTINGS);
			return {
				kind: "missing-toolchain",
				server,
				command: reason.command,
				label: `${label} needs ${reason.code === "no_go" ? "Go" : "curl"}`,
				tone: "warn",
				summary: reason.message,
				detail,
				actions,
			};
		}
		case "offline":
			return {
				kind: "offline",
				server,
				label: "Offline",
				tone: "warn",
				summary: reason.message,
				detail,
				actions: [{ id: "install", label: "Retry install", primary: true }, SETTINGS],
			};
		case "install_failed":
		case "unsupported_platform":
			return {
				kind: "install-failed",
				server,
				label: `${label} install failed`,
				tone: "error",
				summary: reason.message,
				detail,
				actions: [
					...(reason.code === "install_failed"
						? [{ id: "install", label: "Retry install", primary: true } as Action]
						: []),
					LOG,
					SETTINGS,
				],
			};
		case "custom_not_found":
			return {
				kind: "custom-not-found",
				server,
				label: "Custom server not found",
				tone: "error",
				summary: reason.message,
				detail,
				actions: [
					{ ...SETTINGS, primary: true },
					{ id: "retry", label: "Check again" },
				],
			};
		case "outside_root":
			return {
				kind: "outside-root",
				server,
				label: "Outside a project",
				tone: "idle",
				summary: reason.message,
				detail,
				actions: [DOCS],
			};
		case "crashed":
			return crashed(server, label, reason.message, undefined);
	}
}

function crashed(server: string, label: string, message: string, tail: string[] | undefined): LspUiState {
	return {
		kind: "crashed",
		server,
		label: `${label} stopped`,
		tone: "error",
		summary: message || `${label} stopped unexpectedly.`,
		tail: tail?.length ? tail : undefined,
		actions: [{ id: "restart-server", label: "Restart server", primary: true }, LOG, SETTINGS],
	};
}

/** The single state the status item and the feature hints render. */
export function lspUiState(i: UiInput): LspUiState {
	const { host, doc, srv } = i;
	if (!host.lsp) {
		if (host.updating)
			return {
				kind: "host-updating",
				label: "Updating terminal host…",
				tone: "busy",
				summary:
					"The terminal host is updating. Your terminals keep running; language features return in a moment.",
				actions: [],
			};
		const ptys = host.ptys ?? [];
		return {
			kind: "host-old",
			ptys,
			label: "Language features need a newer terminal host",
			tone: "warn",
			summary: "Language features need a newer terminal host. Restart it from here to turn them on.",
			detail: ptys.length
				? `Restarting ends ${ptys.length} running terminal${ptys.length === 1 ? "" : "s"}.`
				: "No terminals are running, so a restart is safe.",
			actions: [
				{ id: "restart-host", label: "Restart terminal host", primary: true },
				{ id: "restart-host-later", label: "Restart later" },
			],
		};
	}

	if (!i.prefsEnabled)
		return {
			kind: "disabled-setting",
			label: "Language servers off",
			tone: "idle",
			summary:
				"Language servers are turned off in settings, so completion, hover and go to definition are unavailable.",
			actions: [{ id: "enable-setting", label: "Turn on", primary: true }, SETTINGS],
		};

	if (i.languageDisabled)
		return {
			kind: "disabled-language",
			label: "Server off for this language",
			tone: "idle",
			summary: `The ${i.languageId} language server is turned off in your settings.`,
			actions: [{ ...SETTINGS, primary: true }],
		};

	if (i.outsideRoot && !srv)
		return {
			kind: "outside-root",
			server: doc?.server ?? "",
			label: "Outside a project",
			tone: "idle",
			summary: "This file is outside every open worktree, so no language server serves it.",
			detail: "Open the folder as a worktree to get language features.",
			actions: [DOCS],
		};

	// `doc` is undefined until hostd answers `lsp_open`. `disabled` is its answer for a language it
	// has no server for; a reply with no server and no failure says the same.
	const noServer = doc && doc.server === null && !srv && doc.status !== "missing" && doc.status !== "crashed";
	if (doc && (doc.status === "disabled" || noServer)) {
		const supported = doc.supported ?? [];
		return {
			kind: "no-language",
			supported,
			label: "No language server",
			tone: "idle",
			summary: `Atlas has no language server for ${i.languageId || "this file type"}.`,
			detail: supported.length ? `Supported: ${supported.join(", ")}.` : undefined,
			actions: [{ ...SETTINGS, primary: true }, DOCS],
		};
	}

	const server = srv?.name ?? doc?.server ?? "";
	const label = srv?.label || pretty(server);
	// The daemon sends English `message` and `hint` plus the `code` and `args` they were written from: show the
	// catalog's sentences (English when the code or an argument is unknown, as from a daemon older than `args`).
	const sent = srv?.reason ?? doc?.reason;
	const reason = sent && { ...sent, ...lspReasonText(sent) };

	if (srv?.state === "downloading") {
		const progress = progressText(srv);
		return {
			kind: "installing",
			server,
			progress,
			label: progress ? `Installing ${label} · ${progress}` : `Installing ${label}…`,
			tone: "busy",
			summary: `Installing ${label}. Language features start as soon as it finishes.`,
			detail: progress || undefined,
			actions: [{ id: "cancel-install", label: "Cancel install" }, LOG],
		};
	}

	if (srv?.state === "crashed" || (!srv && doc?.status === "crashed")) {
		if (reason && reason.code !== "crashed") return fromReason(server, label, reason);
		return crashed(server, label, reason?.message ?? srv?.message ?? "", srv?.tail);
	}

	if (srv?.state === "missing" || (!srv && doc?.status === "missing")) {
		if (reason) return fromReason(server, label, reason);
		return {
			kind: "missing",
			server,
			label: `${label} not installed`,
			tone: "warn",
			summary: srv?.message || `${label} is not installed.`,
			actions: [{ id: "install", label: "Install", primary: true }, SETTINGS],
		};
	}

	if (srv?.state === "ready" || (!srv && doc?.status === "ready"))
		return {
			kind: "ready",
			server,
			version: srv?.version,
			source: srv?.source,
			label: `${label}${srv?.version ? ` ${srv.version}` : ""}`,
			tone: "ok",
			summary: `${label} is running. Completion, diagnostics, hover and go to definition are on.`,
			detail: srv?.source
				? `Source: ${srv.source === "custom" ? "your custom command" : srv.source === "managed" ? "installed by Atlas" : "found on your PATH"}.`
				: undefined,
			actions: [{ id: "restart-server", label: "Restart server" }, LOG, SETTINGS],
		};

	return {
		kind: "starting",
		server,
		label: server ? `Starting ${label}…` : "Starting language server…",
		tone: "busy",
		summary: `Starting ${label}. Language features start as soon as it is ready.`,
		detail: progressText(srv) || undefined,
		actions: [LOG],
	};
}

/** Kinds where a language server is not serving the document (feature shortcuts have nothing to call). */
export const isServing = (s: LspUiState) => s.kind === "ready" || s.kind === "starting";
