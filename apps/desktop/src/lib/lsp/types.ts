/** Wire types shared by the LSP client and the pure UI-state modules (no Svelte, no IPC). */

export type ServerState = "downloading" | "starting" | "ready" | "crashed" | "missing";
export type ServerSource = "path" | "managed" | "custom";

export type ReasonCode =
	| "no_go"
	| "no_curl"
	| "offline"
	| "unsupported_platform"
	| "install_failed"
	| "custom_not_found"
	| "crashed"
	| "outside_root";

/**
 * Why a server is unavailable: a sentence, how to fix it, and a command to run when there is one. `message` and
 * `hint` are the daemon's English; `args` (additive, absent from a daemon older than it) holds the values they were
 * written from, so `lspReasonText` (`#lib/hostText.ts`) can show them in the app's language.
 */
export interface Reason {
	code: ReasonCode;
	message: string;
	hint?: string;
	command?: string;
	args?: Record<string, unknown>;
}

export interface ServerStatus {
	name: string;
	label?: string;
	root: string;
	language: string;
	state: ServerState;
	progress?: string | null;
	message?: string | null;
	version?: string;
	source?: ServerSource;
	/** Absolute path of the server's log file. */
	log?: string;
	/** Last stderr lines. */
	tail?: string[];
	reason?: Reason;
}

export type OpenStatus = "ready" | "missing" | "crashed" | "disabled" | "starting";

export interface CatalogEntry {
	name: string;
	label: string;
	/** LSP language ids this server handles. */
	languages: string[];
	languageLabels: string[];
	install: "github" | "go" | "npm";
	version: string;
	detected: {
		source?: "path" | "managed";
		path?: string;
		version?: string;
		blocked?: Reason;
	};
}

export interface Catalog {
	servers: CatalogEntry[];
	/** Human names of every supported language. */
	supported: string[];
}

/** Per-language server override (user settings or `.atlas/settings.json`). */
export interface LspOverride {
	command?: string;
	args?: string[];
	initialization_options?: Record<string, unknown>;
	settings?: Record<string, unknown>;
	disabled?: boolean;
}
