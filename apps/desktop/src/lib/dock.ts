/**
 * The agent dock (`/dock`): a floating pill that talks to the agents Atlas already runs. Like the tray popover it is
 * a second webview with no workspace of its own: the main window publishes the agents it knows (`buildSnapshot`),
 * Rust caches them and delivers text to their ptys (`dock_send`), so sending works with the main window closed.
 *
 * Everything here is pure (no Svelte, no IPC): the state machine `reduce(state, event) → {state, effects}`, target
 * resolution `resolveTarget`, the snapshot producer and the window sizes. `dock.test.ts` pins them; the snapshot's
 * JSON shape is also read by Rust (`__fixtures__/dock-snapshot.json`, `dock.rs`).
 */

/** What the main window publishes for each agent terminal (camelCase: Rust deserializes the same keys). */
export interface DockAgent {
	tabId: string;
	pty: string;
	/** `omp`, `claude`, `codex` or a custom agent id. */
	agent: string;
	worktreeId: string;
	/** Worktree name (`main`, `feat-login`). */
	worktree: string;
	/** Project name. */
	project: string;
	state: "idle" | "working" | "blocked" | "waiting" | "done";
	lastMessage: string | null;
	updatedAt: number;
}

/** Agents most recently active first (the order of `agents.tabsIn`/`recentTabs`). */
export interface DockSnapshot {
	at: number;
	agents: DockAgent[];
}

export const EMPTY_DOCK_SNAPSHOT: DockSnapshot = { at: 0, agents: [] };

/** The dock settings as Rust serializes them (`dock::DockPrefs`, camelCase). */
export interface DockPrefs {
	enabled: boolean;
	/** Summon key in the keymap's stroke grammar (`ctrl-alt-space`), `""` = none. */
	summon: string;
	/** Hide / show key, `""` = none. Stays registered while the pill is hidden: it is the way back. */
	toggle: string;
}

/** Tab shape `buildSnapshot` needs (a subset of the workspace `Tab`). */
interface TabLike {
	id: string;
	worktreeId: string;
	agent?: string;
	pty?: string;
	chat?: boolean;
}

interface StatusLike {
	state: DockAgent["state"];
	last_message: string | null;
	updated_at: number;
}

/**
 * The snapshot from live stores' data. Only agent terminals with a pty are listed: a plain terminal is not an agent,
 * and an omp chat tab (rpc-ui) takes its input through its protocol, not a bracketed paste. `recent` ranks a tab
 * (lower = more recently active); ties keep workspace order.
 */
export function buildSnapshot(input: {
	now: number;
	tabs: readonly TabLike[];
	worktree: (id: string) => { name: string; project: string } | undefined;
	status: (tabId: string) => Partial<StatusLike> | undefined;
	recent: (tabId: string) => number;
}): DockSnapshot {
	const agents = input.tabs
		.filter((t) => t.agent && t.pty && !t.chat)
		.map((t, i) => ({ t, i, rank: input.recent(t.id) }))
		.sort((a, b) => a.rank - b.rank || a.i - b.i)
		.map(({ t }): DockAgent => {
			const w = input.worktree(t.worktreeId);
			const s = input.status(t.id);
			return {
				tabId: t.id,
				pty: t.pty!,
				agent: t.agent!,
				worktreeId: t.worktreeId,
				worktree: w?.name ?? "",
				project: w?.project ?? "",
				state: s?.state ?? "idle",
				lastMessage: s?.last_message ?? null,
				updatedAt: s?.updated_at ?? 0,
			};
		});
	return { at: input.now, agents };
}

/** `api · omp`, with the worktree in brackets when it is not the project's own (`web · claude (feat-login)`). */
export function agentLabel(a: Pick<DockAgent, "project" | "agent" | "worktree">): string {
	const base = `${a.project || a.worktree} · ${a.agent}`;
	return a.worktree && a.project && a.worktree !== "main" && a.worktree !== a.project
		? `${base} (${a.worktree})`
		: base;
}

export type Resolution =
	| { kind: "target"; tabId: string; text: string }
	/** Several agents match what was typed: the user picks (the dock never guesses between them). */
	| { kind: "ask"; candidates: DockAgent[]; text: string }
	/** Nothing to send (blank, or only a mention). */
	| { kind: "empty"; tabId?: string }
	| { kind: "none"; text: string };

/** `@name` at the start of the text or after whitespace: never the middle of `bob@example.com`. */
const MENTION = /(^|\s)@([\p{L}\p{N}_-]+)[,.:;!?]?(?=\s|$)/gu;

const norm = (s: string) => s.toLowerCase();

function matches(a: DockAgent, token: string): boolean {
	const t = norm(token);
	return [a.worktree, a.project, a.agent].some((f) => norm(f) === t) || norm(agentLabel(a)) === t;
}

/**
 * Who receives `text`, first rule that applies wins: the explicit `chip`, an `@mention` (matches worktree, project
 * or agent kind; stripped from the text when it picks exactly one tab), the `sticky` last used tab, then the most
 * recently active agent (`agents[0]`). Several matches for a mention → `ask`; no agents → `none`.
 */
export function resolveTarget(input: {
	text: string;
	chip?: string | null;
	sticky?: string | null;
	agents: readonly DockAgent[];
}): Resolution {
	const { agents } = input;
	const find = (id?: string | null) => (id ? agents.find((a) => a.tabId === id) : undefined);
	const blank = input.text.trim() === "";
	const chip = find(input.chip);
	if (chip)
		return blank
			? { kind: "empty", tabId: chip.tabId }
			: { kind: "target", tabId: chip.tabId, text: input.text };

	for (const m of input.text.matchAll(MENTION)) {
		const hits = agents.filter((a) => matches(a, m[2]!));
		if (hits.length === 0) continue;
		if (hits.length > 1) return { kind: "ask", candidates: hits, text: input.text };
		const text = input.text
			.replace(m[0], m[1] ?? "")
			.replace(/\s+/g, " ")
			.trim();
		return text === ""
			? { kind: "empty", tabId: hits[0]!.tabId }
			: { kind: "target", tabId: hits[0]!.tabId, text };
	}
	const pick = find(input.sticky) ?? agents[0];
	if (blank) return pick ? { kind: "empty", tabId: pick.tabId } : { kind: "none", text: input.text };
	return pick ? { kind: "target", tabId: pick.tabId, text: input.text } : { kind: "none", text: input.text };
}

/** A file attached to the composer: a capture. */
export interface DockAttachment {
	path: string;
	kind: "screenshot" | "recording" | "gif" | "text";
}

export type Mode = "pill" | "composer" | "ask" | "sending" | "replying" | "needsInput" | "error";

export interface DockState {
	mode: Mode;
	text: string;
	/** Tab the composer's `To:` chip pins; `null` = resolve from the text. */
	chip: string | null;
	/** Captures attached to the message (sent as paths after the text). */
	attachments: DockAttachment[];
	/** The tab the dock last delivered to. */
	sticky: string | null;
	candidates: DockAgent[];
	/** A delivery in flight. */
	pending: { tabId: string; text: string } | null;
	error: string | null;
	/** Tab whose reply is shown, and when it started. */
	replyTab: string | null;
	replySince: number;
	/** Tab asking for input (the card), or one that asked while the composer was busy. */
	needs: string | null;
	queuedNeeds: string | null;
	/** Agents waiting or blocked, for the pill badge. */
	attention: number;
}

export const initialState = (): DockState => ({
	mode: "pill",
	text: "",
	chip: null,
	attachments: [],
	sticky: null,
	candidates: [],
	pending: null,
	error: null,
	replyTab: null,
	replySince: 0,
	needs: null,
	queuedNeeds: null,
	attention: 0,
});

export type DockEvent =
	| { type: "summon" }
	| { type: "esc" }
	| { type: "edit"; text: string }
	| { type: "pin"; tabId: string | null }
	| { type: "send"; agents: readonly DockAgent[] }
	| { type: "choose"; tabId: string }
	| { type: "sendOk"; now?: number }
	| { type: "sendErr"; message: string }
	| { type: "agents"; agents: readonly DockAgent[] }
	| { type: "tick"; now: number; agents: readonly DockAgent[] }
	| { type: "openInAtlas" }
	/** A capture sent from the quick-access card (`dock://attach`). */
	| { type: "attach"; path: string; kind: DockAttachment["kind"] }
	| { type: "detach"; path: string };

export type DockEffect =
	| { type: "focusComposer" }
	| { type: "deliver"; tabId: string; text: string }
	| { type: "openInAtlas"; tabId: string | null };

interface Step {
	state: DockState;
	effects: DockEffect[];
}

/** How long a finished reply stays on screen. */
export const DOCK_REPLY_LINGER_MS = 8_000;

const needsYou = (a: DockAgent) => a.state === "waiting" || a.state === "blocked";

/** Back to the pill, showing a card that was queued while the composer was busy. */
function toPill(s: DockState): DockState {
	const base = {
		...s,
		text: "",
		chip: null,
		attachments: [],
		candidates: [],
		pending: null,
		error: null,
		replyTab: null,
	};
	return s.queuedNeeds
		? { ...base, mode: "needsInput", needs: s.queuedNeeds, queuedNeeds: null }
		: { ...base, mode: "pill", needs: null };
}

const NO_AGENT = "No agent is running. Open Atlas to start one.";

/**
 * The message with the attached files after it. Agents read an image or a clip by path (`@/abs/path` in omp and
 * Claude Code), one per line so the instruction comes first; a path with spaces is quoted so it stays one token.
 */
export function withPaths(text: string, files: readonly DockAttachment[]): string {
	const refs = files.map((f) => (/\s/.test(f.path) ? `@"${f.path}"` : `@${f.path}`));
	return [text.trim(), ...refs].filter((p) => p !== "").join("\n");
}

function deliver(s: DockState, tabId: string, text: string): Step {
	return {
		state: { ...s, mode: "sending", candidates: [], error: null, pending: { tabId, text } },
		effects: [{ type: "deliver", tabId, text }],
	};
}

/** The pure transition function: what happens to the dock on `ev`, and which side effects the page must run. */
export function reduce(s: DockState, ev: DockEvent): Step {
	switch (ev.type) {
		case "summon":
			return s.mode === "composer"
				? { state: toPill(s), effects: [] }
				: {
						state: { ...s, mode: "composer", error: null, candidates: [], needs: null },
						effects: [{ type: "focusComposer" }],
					};
		case "esc":
			return { state: toPill(s), effects: [] };
		case "edit": {
			const back = s.mode === "error" || s.mode === "ask" ? "composer" : s.mode;
			return { state: { ...s, text: ev.text, mode: back, error: null }, effects: [] };
		}
		case "pin":
			return { state: { ...s, chip: ev.tabId }, effects: [] };
		case "send": {
			if (s.text.trim() === "" && s.attachments.length === 0) return { state: s, effects: [] };
			const r = resolveTarget({
				text: s.text.trim() === "" ? "" : s.text,
				chip: s.chip,
				sticky: s.sticky,
				agents: ev.agents,
			});
			switch (r.kind) {
				case "target":
					return deliver(s, r.tabId, withPaths(r.text, s.attachments));
				case "empty":
					// attachments only: there is no text to resolve a mention from, so the chip / sticky / recent
					// target applies (resolveTarget answers `empty` with the tab it would use)
					return r.tabId
						? deliver(s, r.tabId, withPaths("", s.attachments))
						: { state: { ...s, mode: "error", error: NO_AGENT }, effects: [] };
				case "ask":
					return { state: { ...s, mode: "ask", candidates: r.candidates }, effects: [] };
				case "none":
					return { state: { ...s, mode: "error", error: NO_AGENT }, effects: [] };
			}
			return { state: s, effects: [] };
		}
		case "choose": {
			const text = resolveTarget({ text: s.text, chip: ev.tabId, agents: s.candidates }).kind;
			// The mention that was ambiguous is dropped from the text now that the user chose.
			const stripped = s.text.replace(MENTION, "$1").replace(/\s+/g, " ").trim();
			return text === "target"
				? deliver({ ...s, sticky: ev.tabId }, ev.tabId, withPaths(stripped, s.attachments))
				: { state: s, effects: [] };
		}
		case "attach": {
			const have = s.attachments.some((a) => a.path === ev.path);
			const attachments = have ? s.attachments : [...s.attachments, { path: ev.path, kind: ev.kind }];
			const open = s.mode === "composer" || s.mode === "ask";
			return open
				? { state: { ...s, attachments }, effects: [] }
				: {
						state: { ...s, mode: "composer", error: null, candidates: [], needs: null, attachments },
						effects: [{ type: "focusComposer" }],
					};
		}
		case "detach":
			return { state: { ...s, attachments: s.attachments.filter((a) => a.path !== ev.path) }, effects: [] };
		case "sendOk":
			return {
				state: {
					...s,
					mode: "replying",
					text: "",
					chip: null,
					attachments: [],
					pending: null,
					error: null,
					replyTab: s.pending?.tabId ?? null,
					replySince: ev.now ?? 0,
					sticky: s.pending?.tabId ?? s.sticky,
				},
				effects: [],
			};
		case "sendErr":
			return { state: { ...s, mode: "error", error: ev.message, pending: null }, effects: [] };
		case "agents": {
			const attention = ev.agents.filter(needsYou).length;
			const asking = ev.agents.find((a) => a.tabId === s.needs);
			if (s.mode === "needsInput" && !(asking && needsYou(asking)))
				return { state: { ...toPill(s), attention }, effects: [] };
			const fresh = ev.agents.find((a) => needsYou(a) && a.tabId !== s.needs);
			if (!fresh) return { state: { ...s, attention }, effects: [] };
			// Never interrupt what the user is doing: a busy dock only remembers the card.
			return s.mode === "pill" || s.mode === "replying"
				? { state: { ...s, mode: "needsInput", needs: fresh.tabId, attention }, effects: [] }
				: { state: { ...s, queuedNeeds: fresh.tabId, attention }, effects: [] };
		}
		case "tick": {
			if (s.mode !== "replying") return { state: s, effects: [] };
			const a = ev.agents.find((x) => x.tabId === s.replyTab);
			const settled = !a || a.state === "done" || a.state === "idle";
			return settled && ev.now - s.replySince > DOCK_REPLY_LINGER_MS
				? { state: toPill(s), effects: [] }
				: { state: s, effects: [] };
		}
		case "openInAtlas":
			return {
				state: toPill(s),
				effects: [{ type: "openInAtlas", tabId: s.needs ?? s.replyTab ?? s.pending?.tabId ?? null }],
			};
	}
}

/** Lines shown in the reply preview. */
const REPLY_LINES = 6;

/** The reply preview of an agent: the tail of its last message, and whether it is still working. */
export function replyOf(a: Pick<DockAgent, "lastMessage" | "state">): { lines: string[]; busy: boolean } {
	const lines = (a.lastMessage ?? "")
		.split("\n")
		.map((l) => l.trimEnd())
		.filter((l) => l !== "");
	return { lines: lines.slice(-REPLY_LINES), busy: a.state === "working" };
}

/** Logical px of the window for each mode (the page reports its measured height on top of this minimum). */
export function windowSize(mode: Mode): { width: number; height: number } {
	switch (mode) {
		case "pill":
			return { width: 320, height: 40 };
		case "sending":
			return { width: 360, height: 44 };
		case "composer":
		case "ask":
		case "error":
			return { width: 520, height: 150 };
		case "replying":
		case "needsInput":
			return { width: 520, height: 140 };
	}
}
