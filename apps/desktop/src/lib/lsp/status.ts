/**
 * What the editor tells the user about language features for one file. A pure reducer from the
 * facts (settings, host, the document's open reply, the live server status) to one state with a
 * chip label, a sentence, optional detail and the actions that fix it. Rendering and IPC live
 * elsewhere (`LspStatus.svelte`, `client.svelte.ts`).
 */
import { list } from "#lib/i18n/fmt.ts";
import { m } from "#lib/i18n/m.ts";
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

// The three actions many states offer: read when a state is built, so they follow the app's language.
const settings = (): Action => ({ id: "open-settings-languages", label: m.workbench_lsp_action_settings() });
const log = (): Action => ({ id: "open-log", label: m.workbench_lsp_action_log() });
const docs = (): Action => ({ id: "docs", label: m.workbench_lsp_action_docs() });

/** Install progress: the server sends free text (`Downloading… 42%`); keep it as is, trimmed. */
const progressText = (srv: ServerStatus | undefined) => (srv?.progress || srv?.message || "").trim();

function fromReason(server: string, label: string, reason: Reason): LspUiState {
	const detail = reason.hint;
	switch (reason.code) {
		case "no_go":
		case "no_curl": {
			const actions: Action[] = [];
			if (reason.command)
				actions.push({ id: "copy-command", label: m.viewers_devops_copy_command(), payload: reason.command });
			actions.push(
				{ id: "retry", label: m.crew_onboarding_check_again(), primary: !reason.command },
				settings(),
			);
			return {
				kind: "missing-toolchain",
				server,
				command: reason.command,
				label: m.workbench_lsp_needs_tool({ label, tool: reason.code === "no_go" ? "Go" : "curl" }),
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
				label: m.workbench_lsp_offline(),
				tone: "warn",
				summary: reason.message,
				detail,
				actions: [
					{ id: "install", label: m.workbench_lsp_action_retry_install(), primary: true },
					settings(),
				],
			};
		case "install_failed":
		case "unsupported_platform":
			return {
				kind: "install-failed",
				server,
				label: m.workbench_lsp_install_failed({ label }),
				tone: "error",
				summary: reason.message,
				detail,
				actions: [
					...(reason.code === "install_failed"
						? [{ id: "install", label: m.workbench_lsp_action_retry_install(), primary: true } as Action]
						: []),
					log(),
					settings(),
				],
			};
		case "custom_not_found":
			return {
				kind: "custom-not-found",
				server,
				label: m.workbench_lsp_custom_not_found(),
				tone: "error",
				summary: reason.message,
				detail,
				actions: [
					{ ...settings(), primary: true },
					{ id: "retry", label: m.crew_onboarding_check_again() },
				],
			};
		case "outside_root":
			return {
				kind: "outside-root",
				server,
				label: m.workbench_lsp_outside_project(),
				tone: "idle",
				summary: reason.message,
				detail,
				actions: [docs()],
			};
		case "crashed":
			return crashed(server, label, reason.message, undefined);
	}
}

function crashed(server: string, label: string, message: string, tail: string[] | undefined): LspUiState {
	return {
		kind: "crashed",
		server,
		label: m.workbench_lsp_crashed({ label }),
		tone: "error",
		summary: message || m.workbench_lsp_crashed_summary({ label }),
		tail: tail?.length ? tail : undefined,
		actions: [
			{ id: "restart-server", label: m.workbench_lsp_action_restart_server(), primary: true },
			log(),
			settings(),
		],
	};
}

/** The single state the status item and the feature hints render. */
export function lspUiState(i: UiInput): LspUiState {
	const { host, doc, srv } = i;
	if (!host.lsp) {
		if (host.updating)
			return {
				kind: "host-updating",
				label: m.workbench_host_chip_updating(),
				tone: "busy",
				summary: m.workbench_lsp_host_updating(),
				actions: [],
			};
		const ptys = host.ptys ?? [];
		return {
			kind: "host-old",
			ptys,
			label: m.workbench_lsp_host_old_label(),
			tone: "warn",
			summary: m.workbench_lsp_host_old_summary(),
			detail: ptys.length
				? m.workbench_lsp_host_old_ends({ count: ptys.length })
				: m.workbench_lsp_host_old_safe(),
			actions: [
				{ id: "restart-host", label: m.workbench_term_pane_restart_host(), primary: true },
				{ id: "restart-host-later", label: m.workbench_lsp_action_restart_later() },
			],
		};
	}

	if (!i.prefsEnabled)
		return {
			kind: "disabled-setting",
			label: m.workbench_lsp_off_label(),
			tone: "idle",
			summary: m.workbench_lsp_off_summary(),
			actions: [{ id: "enable-setting", label: m.workbench_lsp_action_turn_on(), primary: true }, settings()],
		};

	if (i.languageDisabled)
		return {
			kind: "disabled-language",
			label: m.workbench_lsp_language_off_label(),
			tone: "idle",
			summary: m.workbench_lsp_language_off_summary({ language: i.languageId }),
			actions: [{ ...settings(), primary: true }],
		};

	if (i.outsideRoot && !srv)
		return {
			kind: "outside-root",
			server: doc?.server ?? "",
			label: m.workbench_lsp_outside_project(),
			tone: "idle",
			summary: m.workbench_lsp_outside_summary(),
			detail: m.workbench_lsp_outside_detail(),
			actions: [docs()],
		};

	// `doc` is undefined until hostd answers `lsp_open`. `disabled` is its answer for a language it
	// has no server for; a reply with no server and no failure says the same.
	const noServer = doc && doc.server === null && !srv && doc.status !== "missing" && doc.status !== "crashed";
	if (doc && (doc.status === "disabled" || noServer)) {
		const supported = doc.supported ?? [];
		return {
			kind: "no-language",
			supported,
			label: m.workbench_lsp_none_label(),
			tone: "idle",
			summary: i.languageId
				? m.workbench_lsp_none_summary({ language: i.languageId })
				: m.workbench_lsp_none_summary_file_type(),
			detail: supported.length ? m.workbench_lsp_none_supported({ list: list(supported) }) : undefined,
			actions: [{ ...settings(), primary: true }, docs()],
		};
	}

	const server = srv?.name ?? doc?.server ?? "";
	const label = srv?.label || server || m.workbench_editor_language_server();
	const reason = srv?.reason ?? doc?.reason;

	if (srv?.state === "downloading") {
		const progress = progressText(srv);
		return {
			kind: "installing",
			server,
			progress,
			label: progress
				? m.workbench_lsp_installing_progress({ label, progress })
				: m.workbench_lsp_installing({ label }),
			tone: "busy",
			summary: m.workbench_lsp_installing_summary({ label }),
			detail: progress || undefined,
			actions: [{ id: "cancel-install", label: m.workbench_lsp_action_cancel_install() }, log()],
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
			label: m.workbench_lsp_not_installed({ label }),
			tone: "warn",
			summary: srv?.message || m.workbench_lsp_not_installed_summary({ label }),
			actions: [
				{ id: "install", label: m.crew_onboarding_engine_status_install(), primary: true },
				settings(),
			],
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
			summary: m.workbench_lsp_running_summary({ label }),
			detail: srv?.source
				? srv.source === "custom"
					? m.workbench_lsp_source_custom()
					: srv.source === "managed"
						? m.workbench_lsp_source_managed()
						: m.workbench_lsp_source_path()
				: undefined,
			actions: [{ id: "restart-server", label: m.workbench_lsp_action_restart_server() }, log(), settings()],
		};

	return {
		kind: "starting",
		server,
		label: server ? m.workbench_lsp_starting({ label }) : m.workbench_lsp_starting_generic(),
		tone: "busy",
		summary: m.workbench_lsp_starting_summary({ label }),
		detail: progressText(srv) || undefined,
		actions: [log()],
	};
}

/** Kinds where a language server is not serving the document (feature shortcuts have nothing to call). */
export const isServing = (s: LspUiState) => s.kind === "ready" || s.kind === "starting";
