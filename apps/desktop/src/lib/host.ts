import { invoke } from "@tauri-apps/api/core";

/** Raw passthrough to `atlas hostd` (see crates/atlas-host/src/protocol.rs). */
export const hostCall = <T = unknown>(cmd: string, args: Record<string, unknown> = {}) =>
	invoke<T>("host_call", { cmd, args });

export interface PtyScope {
	worktree_id?: string;
	tab_id?: string;
	pane_key?: string;
}

export interface PtySummary {
	pty: string;
	cwd: string;
	argv: string[];
	scope: PtyScope;
	title: string;
	created_at: number;
	exited: { code: number | null } | null;
	pid: number | null;
	fg: { pid: number; name: string } | null;
}

export interface ExecResult {
	code: number | null;
	stdout: string;
	stderr: string;
	timed_out: boolean;
}

export interface FsEntry {
	name: string;
	kind: "file" | "dir" | "symlink";
	size: number;
	mtime: number;
}

const enc = new TextEncoder();
const dec = new TextDecoder();

export function b64encode(s: string | Uint8Array): string {
	const bytes = typeof s === "string" ? enc.encode(s) : s;
	let bin = "";
	for (let i = 0; i < bytes.length; i += 0x8000) bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
	return btoa(bin);
}

export function b64bytes(b: string): Uint8Array {
	const bin = atob(b);
	const out = new Uint8Array(bin.length);
	for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
	return out;
}

export const b64decode = (b: string) => dec.decode(b64bytes(b));

export const host = {
	ptyCreate: (a: {
		cwd: string;
		argv?: string[];
		env?: Record<string, string>;
		cols?: number;
		rows?: number;
		scope?: PtyScope;
		title?: string;
		/** `pipe`: stdin/stdout line protocol instead of a terminal. */
		mode?: "pty" | "pipe";
	}) => hostCall<{ pty: string }>("pty_create", a),
	/** `output: false`: title/exit events only (the GPU terminal reads a raw stream). */
	ptyAttach: (pty: string, replay = true, output = true) =>
		hostCall<{ cols: number; rows: number; title: string; exited: { code: number | null } | null }>(
			"pty_attach",
			{ pty, replay, output },
		),
	ptyDetach: (pty: string) => hostCall("pty_detach", { pty }),
	ptyWrite: (pty: string, text: string | Uint8Array) => hostCall("pty_write", { pty, data: b64encode(text) }),
	ptyResize: (pty: string, cols: number, rows: number) => hostCall("pty_resize", { pty, cols, rows }),
	ptyKill: (pty: string) => hostCall("pty_kill", { pty }),
	/** Terminals of every connected host (local + SSH). */
	ptyList: () => invoke<PtySummary[]>("host_pty_list"),
	ptyInfo: (pty: string) => hostCall<PtySummary & { cwd_now: string | null }>("pty_info", { pty }),
	ptyRead: (pty: string, lines = 200) =>
		hostCall<{ text: string }>("pty_read", { pty, lines }).then((r) => r.text),
	/** Runs a command; stdout/stderr come back decoded. Throws on spawn errors only. */
	exec: async (
		cwd: string,
		argv: string[],
		opts: { stdin?: string; timeout_ms?: number; env?: Record<string, string> } = {},
	) => {
		const r = await hostCall<ExecResult>("exec", {
			cwd,
			argv,
			env: opts.env ?? {},
			stdin: opts.stdin === undefined ? null : b64encode(opts.stdin),
			timeout_ms: opts.timeout_ms ?? 60_000,
		});
		return { ...r, stdout: b64decode(r.stdout), stderr: b64decode(r.stderr) };
	},
	fsList: (path: string) => hostCall<{ entries: FsEntry[] }>("fs_list", { path }).then((r) => r.entries),
	fsRead: (path: string, max?: number) =>
		hostCall<{ data: string; truncated: boolean }>("fs_read", { path, max }),
	fsWrite: (path: string, text: string) => hostCall("fs_write", { path, data: b64encode(text) }),
	fsStat: (path: string) => hostCall<{ kind: string; size: number; mtime: number }>("fs_stat", { path }),
	fsMkdir: (path: string) => hostCall("fs_mkdir", { path }),
	fsRename: (from: string, to: string) => hostCall("fs_rename", { from, to }),
	fsRemove: (path: string, recursive = false) => hostCall("fs_remove", { path, recursive }),
	fsWatch: (path: string, id: string) => hostCall("fs_watch", { path, id }),
	fsUnwatch: (id: string) => hostCall("fs_unwatch", { id }),
	info: () =>
		hostCall<{ version: string; pid: number; uptime_s: number; ptys: number; hostname: string }>("info"),
};
