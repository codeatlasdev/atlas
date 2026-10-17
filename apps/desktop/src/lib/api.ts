import { invoke } from "@tauri-apps/api/core";
import type { Found } from "#lib/components/sidebar/discovery.ts";
import type { Identity } from "#lib/shell/identity.ts";
import type { ProviderState } from "#lib/commitMessage.ts";
import { parseBranches, parseSwitched, type Branches, type Switched } from "#lib/branches.ts";

export interface DiscoverArgs {
	/** Extra folders besides the defaults (absolute or `~/…`). */
	roots?: string[];
	/** Scan only `roots`. */
	only_roots?: boolean;
	/** Also report git roots without atlas.yaml. */
	candidates?: boolean;
	depth?: number;
}

export interface DiscoverDone {
	roots: string[];
	dirs: number;
	unreadable: number;
	elapsed_ms: number;
	cancelled: boolean;
}

export interface ProjectView {
	id: string;
	root: string;
	name: string;
	pinned: boolean;
	running: boolean;
	mode: string | null;
	services_total: number;
	services_healthy: number;
	error: string | null;
	profiles: string[];
	/** Linked worktree: id of the main checkout. */
	parent: string | null;
	port_offset: number;
	branch: string | null;
	/** `ssh:<alias>` for checkouts on an SSH host. */
	host: string | null;
	/** User-chosen icon and color; null = automatic. */
	identity: Identity | null;
	/** The checkout is gone from this machine (null = present, or an SSH project). */
	missing: "root" | "config" | null;
	/** `origin` URL last seen while the project existed (set when missing). */
	last_remote: string | null;
}

export interface WorktreeProgress {
	creation_id: string;
	step: "fetch" | "add" | "shared" | "done" | "error";
	message?: string;
	worktree?: {
		id: string;
		path: string;
		branch: string;
		base: string;
		portOffset: number;
		repoId: string;
		name: string;
		linkedPr: number | null;
	};
	setup?: { argv: string[]; env: Record<string, string> } | null;
}

export interface WorktreeStats {
	kb: number | null;
	dirty: boolean | null;
	ahead: number | null;
	behind: number | null;
	base: string | null;
	last_commit: number | null;
}

export type ServiceKind = "web" | "api" | "database" | "cache" | "worker" | "task";

export interface ServiceDef {
	name: string;
	port: number | null;
	enabled: boolean;
	kind: ServiceKind;
	/** `open_key` from atlas.yaml: a bare key that opens the service (same as the TUI). */
	open_key: string | null;
	/** Browser URL for web/api, connection URL (copied) for database/cache. */
	target: { how: "browser" | "copy"; url: string } | null;
	command: string | null;
	dir: string | null;
	/** Services this one waits for, in declaration order. */
	depends_on: string[];
}

interface ActionDef {
	key: string;
	label: string;
	group: string | null;
	confirm: string | null;
	session: boolean;
}

/** Dependency-install step (`pre_command` or the auto-detected installer). */
export interface SetupConfig {
	/** Effective command; null when there is nothing to run. */
	command: string | null;
	/** `always`: every start; `lockfile`: only when node_modules is missing/stale. */
	when: "always" | "lockfile";
	/** No `pre_command` in atlas.yaml: the installer was auto-detected. */
	auto: boolean;
}

/** `.logs/setup.json`: state of the last run. */
interface SetupRun {
	state: "running" | "ok" | "failed";
	command: string;
	started_at: number;
	finished_at?: number;
	reason?: string;
}

export interface SetupStatus {
	status: SetupRun | null;
	/** Last 200 lines of `.logs/setup.log`. */
	output: string;
}

/** What the Add flow learns about a picked folder (`project_probe`). */
export interface Probe {
	picked: string;
	/** Where `atlas.yaml` is (the picked folder or one above it); the picked folder when there is none. */
	root: string;
	has_config: boolean;
	name: string;
	/** Id of the project already registered for this folder. */
	registered: string | null;
}

/** What `atlas init` would write in a folder (`project_init_preview`). */
export interface InitPreview {
	yaml: string;
	detected: string | null;
	services: string[];
}

export interface ProjectConfig {
	name: string;
	services: ServiceDef[];
	actions: ActionDef[];
	setup: SetupConfig;
}

export interface Problem {
	path: string;
	message: string;
}

type ServiceCmd = "start_service" | "stop_service" | "restart_service";

export interface ShellInfo {
	configured: string | null;
	installed: string[];
	probe: {
		shell: string;
		source: "settings" | "account" | "$SHELL" | "default";
		flags: string | null;
		error: string | null;
		took_ms: number;
	} | null;
	path: string[];
	tools: [string, string | null][];
}

export const api = {
	list: () => invoke<ProjectView[]>("projects_list"),
	add: (root: string) => invoke<ProjectView>("projects_add", { root }),
	resync: (id: string) => invoke<void>("live_resync", { id }),
	remove: (id: string) => invoke<void>("projects_remove", { id }),
	pin: (id: string, pinned: boolean) => invoke<void>("projects_pin", { id, pinned }),
	/** `null` (or an empty identity) goes back to the automatic icon and color. */
	setIdentity: (id: string, identity: Identity | null) =>
		invoke<void>("projects_set_identity", { id, identity }),
	discover: (args: DiscoverArgs = {}) => invoke<DiscoverDone>("projects_discover", { args }),
	discoverCancel: () => invoke<void>("projects_discover_cancel"),
	relink: (id: string, root: string) => invoke<ProjectView>("projects_relink", { id, root }),
	removeMissing: () => invoke<number>("projects_remove_missing"),
	findMoved: (id: string) => invoke<Found | null>("projects_find_moved", { id }),
	initProject: (dir: string) => invoke<ProjectView>("project_init", { dir }),
	probe: (dir: string) => invoke<Probe>("project_probe", { dir }),
	initPreview: (dir: string) => invoke<InitPreview>("project_init_preview", { dir }),
	start: (id: string, profiles: string[] = [], services: string[] = []) =>
		invoke<void>("stack_start", { id, services, profiles }),
	stop: (id: string) => invoke<void>("stack_stop", { id }),
	service: (id: string, cmd: ServiceCmd, service: string) =>
		invoke<unknown>("service_cmd", { id, cmd, service }),
	action: (id: string, target: string, confirmed: boolean) =>
		invoke<{ needs_confirm?: string; label?: string; action?: string }>("action_run", {
			id,
			target,
			confirmed,
		}),
	projectConfig: (id: string) => invoke<ProjectConfig>("project_config", { id }),
	setupStatus: (id: string) => invoke<SetupStatus>("setup_status", { id }),
	setupRun: (id: string) => invoke<{ ok: boolean; reason?: string; skipped?: boolean }>("setup_run", { id }),
	configRead: (id: string) => invoke<{ text: string; path: string }>("config_read", { id }),
	configSchema: () => invoke<string>("config_schema"),
	configValidate: (id: string, text: string) => invoke<Problem[]>("config_validate", { id, text }),
	configWrite: (id: string, text: string) => invoke<void>("config_write", { id, text }),
	openEditor: (id: string) => invoke<void>("open_in_editor", { id }),
	openUrl: (url: string) => invoke<void>("open_url", { url }),
	reveal: (id: string) => invoke<void>("reveal", { id }),
	shellInfo: () => invoke<ShellInfo>("shell_info"),
	shellSet: (shell: string | null) => invoke<ShellInfo>("shell_set", { shell }),
	settings: () => invoke<Record<string, unknown>>("settings_get"),
	settingsPatch: (patch: Record<string, unknown>) =>
		invoke<Record<string, unknown>>("settings_patch", { patch }),
	worktreeCreate: (args: {
		repoId: string;
		name?: string;
		base?: string;
		branch?: string;
		runSetup?: boolean;
		pr?: number;
	}) => invoke<{ creation_id: string }>("worktree_create", { args }),
	worktreeRemove: (id: string, force: boolean) =>
		invoke<{ preserved_branch: string | null }>("worktree_remove", { id, force }),
	gitRefs: (id: string) => invoke<string[]>("git_refs", { id }),
	worktreesImport: (repoId: string) => invoke<number>("worktrees_import", { repoId }),
	worktreeStats: (id: string) => invoke<WorktreeStats>("worktree_stats", { id }),
};

/** Tauri rejects with the command's error string. */
export function errText(e: unknown): string {
	return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}

interface DiffLine {
	kind: "ctx" | "add" | "del";
	text: string;
	old_no?: number;
	new_no?: number;
}
export interface DiffHunk {
	header: string;
	old_start: number;
	old_lines: number;
	new_start: number;
	new_lines: number;
	lines: DiffLine[];
}
export interface FileDiff {
	path: string;
	old_path?: string;
	status: "modified" | "added" | "deleted" | "renamed" | "untracked";
	staged: boolean;
	binary: boolean;
	hunks: DiffHunk[];
}
export interface GitEntry {
	path: string;
	orig?: string;
	x: string;
	y: string;
	conflict?: boolean;
}
export interface GitStatus {
	branch: string | null;
	upstream: string | null;
	ab: string | null;
	base: string;
	vs_base: { ahead: number; behind: number } | null;
	entries: GitEntry[];
	operation: "merge" | "rebase" | null;
}
interface RunResult {
	code: number | null;
	stdout: string;
	stderr: string;
}

export const git = {
	diff: (id: string, mode: "working" | "staged" | "base", base?: string) =>
		invoke<{ files: FileDiff[]; base: string | null }>("git_diff", { id, mode, base: base ?? null }),
	status: (id: string) => invoke<GitStatus>("git_status", { id }),
	run: (id: string, args: string[], stdin?: string) =>
		invoke<RunResult>("git_run", { id, args, stdin: stdin ?? null }),
	files: (id: string) => invoke<string[]>("git_files", { id }),
	commitMsgProvider: () => invoke<ProviderState>("commit_msg_provider"),
	/** Starts a run; progress arrives as `commit-msg://<request>` events. Rejects with `no_provider` when none is set up. */
	commitMsgGenerate: (id: string, instruction?: string) =>
		invoke<string>("commit_msg_generate", { id, instruction: instruction ?? null }),
	commitMsgCancel: (request: string) => invoke<void>("commit_msg_cancel", { request }),
	branches: async (id: string): Promise<Branches> => parseBranches(await invoke("git_branches", { id })),
	/** `create`: new branch from HEAD; `stash`: stash (incl. untracked) first. */
	switch: async (
		id: string,
		branch: string,
		opts: { create?: boolean; stash?: boolean } = {},
	): Promise<Switched> =>
		parseSwitched(
			await invoke("git_switch", { id, req: { branch, create: !!opts.create, stash: !!opts.stash } }),
		),
};
