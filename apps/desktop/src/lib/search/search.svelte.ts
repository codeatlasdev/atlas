import { host } from "#lib/host.ts";
import { errText } from "#lib/api.ts";
import { ws } from "#lib/stores/workspace.svelte.ts";
import {
	Results,
	gitGrepArgs,
	locator,
	rgArgs,
	rgError,
	type FileResult,
	type SearchOpts,
} from "#lib/search/parse.ts";

const DEBOUNCE_MS = 220;
const TIMEOUT_MS = 20_000;

export interface SearchState extends SearchOpts {
	showFilters: boolean;
	phase: "idle" | "running" | "done" | "error";
	files: FileResult[];
	matches: number;
	truncated: boolean;
	error: string;
	tool: "rg" | "git" | "";
	tookMs: number;
}

const fresh = (): SearchState => ({
	query: "",
	regex: false,
	caseSensitive: false,
	wholeWord: false,
	include: "",
	exclude: "",
	showFilters: false,
	phase: "idle",
	files: [],
	matches: 0,
	truncated: false,
	error: "",
	tool: "",
	tookMs: 0,
});

/** Search state per worktree, kept while the panel is hidden. */
const states: Record<string, SearchState> = $state({});
export const searchState = (id: string): SearchState => (states[id] ??= fresh());

const generation: Record<string, number> = {};
const timers: Record<string, number> = {};
/** Input element of the mounted panel, for ⌘⇧F. */
let input: HTMLInputElement | undefined;
export const registerSearchInput = (el: HTMLInputElement | undefined) => (input = el);
export function focusSearch() {
	queueMicrotask(() => {
		input?.focus();
		input?.select();
	});
}

const optsOf = (s: SearchState): SearchOpts => ({
	query: s.query,
	regex: s.regex,
	caseSensitive: s.caseSensitive,
	wholeWord: s.wholeWord,
	include: s.include,
	exclude: s.exclude,
});

/** Debounced search; every call supersedes the previous one (its result is dropped). */
export function scheduleSearch(id: string, immediate = false) {
	clearTimeout(timers[id]);
	const s = searchState(id);
	if (!s.query) {
		generation[id] = (generation[id] ?? 0) + 1;
		Object.assign(s, { phase: "idle", files: [], matches: 0, truncated: false, error: "" });
		return;
	}
	timers[id] = window.setTimeout(() => void runSearch(id), immediate ? 0 : DEBOUNCE_MS);
}

let rgMissing: boolean | undefined;

async function runSearch(id: string) {
	const s = searchState(id);
	const root = ws.worktree(id)?.path;
	if (!root) return;
	const gen = (generation[id] = (generation[id] ?? 0) + 1);
	const opts = optsOf(s);
	const started = performance.now();
	s.phase = "running";
	s.error = "";
	try {
		let tool: "rg" | "git" = rgMissing ? "git" : "rg";
		let r = await host.exec(root, tool === "rg" ? rgArgs(opts) : gitGrepArgs(opts), {
			timeout_ms: TIMEOUT_MS,
		});
		// rg not installed: remember it and fall back to git grep (tracked + untracked, honoring .gitignore).
		if (tool === "rg" && r.code === 127) {
			rgMissing = true;
			tool = "git";
			r = await host.exec(root, gitGrepArgs(opts), { timeout_ms: TIMEOUT_MS });
		}
		if (gen !== generation[id]) return;
		const res = new Results();
		const locate = locator(opts);
		for (const line of r.stdout.split("\n")) {
			if (!(tool === "rg" ? res.addRgLine(line) : res.addGrepLine(line, locate))) break;
		}
		const err = tool === "rg" ? rgError(r.code, r.stderr) : r.code && r.code > 1 ? r.stderr.trim() : null;
		if (err) {
			Object.assign(s, { phase: "error", error: err, files: [], matches: 0, truncated: false, tool });
			return;
		}
		Object.assign(s, {
			phase: "done",
			files: res.files,
			matches: res.matches,
			truncated: res.truncated || r.timed_out,
			tool,
			tookMs: Math.round(performance.now() - started),
		});
	} catch (e) {
		if (gen !== generation[id]) return;
		// hostd reports a missing binary as a spawn error rather than exit 127.
		if (!rgMissing && /No such file|not found|ENOENT|os error 2/i.test(errText(e))) {
			rgMissing = true;
			void runSearch(id);
			return;
		}
		Object.assign(s, { phase: "error", error: errText(e), files: [], matches: 0, truncated: false });
	}
}
