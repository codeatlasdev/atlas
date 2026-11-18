/**
 * Pure model behind Settings → Editor → Languages: what state a language server is in, how a
 * JSON text box is validated and how an edit becomes a patch of the `lsp` preference.
 * No Svelte, no IPC.
 */
import { m } from "#lib/i18n/m.ts";
import type { LspLanguagePref } from "#lib/prefs.ts";
import type { CatalogEntry, ServerStatus } from "#lib/lsp/types.ts";

export type ServerView =
	| { kind: "downloading"; text: string }
	| { kind: "starting"; text: string }
	| { kind: "running"; text: string }
	| { kind: "crashed"; text: string }
	| { kind: "path"; text: string }
	| { kind: "managed"; text: string }
	| { kind: "blocked"; text: string }
	| { kind: "missing"; text: string };

/** Servers of `entry` that the host is running or installing (one per project root). */
export function statusesOf(entry: CatalogEntry, servers: readonly ServerStatus[]): ServerStatus[] {
	return servers.filter((s) => s.name === entry.name);
}

const RANK: Record<ServerStatus["state"], number> = {
	crashed: 0,
	downloading: 1,
	starting: 2,
	ready: 3,
	missing: 4,
};

/** The most important live status: a crash beats a download beats a running server. */
export function headline(statuses: readonly ServerStatus[]): ServerStatus | undefined {
	return [...statuses].sort((a, b) => RANK[a.state] - RANK[b.state])[0];
}

/**
 * One state for the row: live status first (installing, running, crashed), then what is on
 * disk (PATH or managed), then why it cannot be installed, else "Not installed".
 */
export function serverView(entry: CatalogEntry, servers: readonly ServerStatus[]): ServerView {
	const live = headline(statusesOf(entry, servers));
	// A progress line or a reason that came from the host is shown as it arrived (the host writes it, not this page).
	if (live?.state === "downloading")
		return { kind: "downloading", text: live.progress || m.settings_editor_view_downloading() };
	if (live?.state === "starting")
		return { kind: "starting", text: live.progress || m.settings_editor_view_starting() };
	if (live?.state === "crashed") {
		return {
			kind: "crashed",
			text: live.reason?.message || live.message || m.settings_editor_view_stopped(),
		};
	}
	if (live?.state === "ready") return { kind: "running", text: m.settings_editor_view_running() };
	const { detected } = entry;
	if (detected.source === "path") return { kind: "path", text: m.settings_editor_view_installed() };
	if (detected.source === "managed") return { kind: "managed", text: m.settings_editor_view_installed() };
	if (detected.blocked) return { kind: "blocked", text: detected.blocked.message };
	return { kind: "missing", text: m.settings_editor_view_not_installed() };
}

/** Where the executable comes from, for the badge. */
export function sourceBadge(entry: CatalogEntry, servers: readonly ServerStatus[]): string | null {
	const live = headline(statusesOf(entry, servers));
	const source = live?.source ?? entry.detected.source;
	if (source === "path") return "PATH";
	if (source === "managed") return m.settings_editor_badge_managed();
	if (source === "custom") return m.settings_editor_badge_custom();
	return null;
}

/** Version to show: the running server's, else the installed one. */
export function versionOf(entry: CatalogEntry, servers: readonly ServerStatus[]): string | null {
	const live = headline(statusesOf(entry, servers));
	return live?.version || entry.detected.version || null;
}

/** Only a server Atlas installed can be removed (or updated) from here; a PATH install is the user's. */
export const canUninstall = (entry: CatalogEntry): boolean => entry.detected.source === "managed";

/** The install is running now: the buttons that would start another one are disabled. */
export function isBusy(entry: CatalogEntry, servers: readonly ServerStatus[]): boolean {
	return statusesOf(entry, servers).some((s) => s.state === "downloading");
}

export type JsonField =
	{ ok: true; value: Record<string, unknown> | undefined } | { ok: false; error: string };

/** Parse the text of an options box: empty clears it, anything else must be a JSON object. */
export function parseJsonObject(text: string): JsonField {
	const t = text.trim();
	if (t === "") return { ok: true, value: undefined };
	let v: unknown;
	try {
		v = JSON.parse(t);
	} catch (e) {
		const msg = e instanceof Error ? e.message : String(e);
		return { ok: false, error: m.settings_editor_json_not_valid({ message: msg }) };
	}
	if (!v || typeof v !== "object" || Array.isArray(v)) {
		return { ok: false, error: m.settings_editor_json_not_object({ example: '{ "key": "value" }' }) };
	}
	return { ok: true, value: v as Record<string, unknown> };
}

/** Pretty text for an options box. */
export const jsonText = (v: Record<string, unknown> | undefined): string =>
	v && Object.keys(v).length > 0 ? JSON.stringify(v, null, 2) : "";

/**
 * The `lsp` map with `language` replaced by `next` merged over its current entry.
 * A key set to `undefined` is removed; an entry left empty is removed from the map.
 */
export function patchLanguage(
	lsp: Record<string, LspLanguagePref>,
	language: string,
	next: { [K in keyof LspLanguagePref]?: LspLanguagePref[K] | undefined },
): Record<string, LspLanguagePref> {
	const merged: Record<string, unknown> = { ...lsp[language], ...next };
	for (const k of Object.keys(merged)) if (merged[k] === undefined) delete merged[k];
	const { [language]: _old, ...rest } = lsp;
	return Object.keys(merged).length > 0 ? { ...rest, [language]: merged as LspLanguagePref } : rest;
}

/** Does `language` have a custom command, args or options (what "Reset to default" removes)? */
export function hasCustom(pref: LspLanguagePref | undefined): boolean {
	return (
		!!pref &&
		(pref.command !== undefined ||
			pref.args !== undefined ||
			pref.initialization_options !== undefined ||
			pref.settings !== undefined)
	);
}

/** A custom command that cannot be right: it is set but the program name is empty or has a space and no args. */
export function commandProblem(command: string): string | null {
	const c = command.trim();
	if (c === "") return null;
	if (/\s/.test(c) && !/^["'\/~]/.test(c)) {
		return m.settings_editor_command_problem();
	}
	return null;
}
