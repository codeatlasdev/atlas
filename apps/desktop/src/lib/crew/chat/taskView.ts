/**
 * Owns: pure display helpers for the task thread: time/size formatting, markdown collapse, the approval-request
 * summary, date separators and the "who started this task" label. No Svelte, no stores.
 */

/** A reply longer than this many lines is folded behind "Show more". */
export const COLLAPSE_LINES = 12;

export function fmtTime(tsMs: number): string {
	return new Date(tsMs).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

export function fmtElapsed(ms: number): string {
	const s = Math.floor(ms / 1000);
	if (s < 60) return `${s}s`;
	const m = Math.floor(s / 60);
	if (m < 60) return `${m}m`;
	const h = Math.floor(m / 60);
	return h < 24 ? `${h}h ${m % 60}m` : `${Math.floor(h / 24)}d ${h % 24}h`;
}

export function fmtTokens(n: number): string {
	if (n >= 1000) return `${(n / 1000).toFixed(1)}k`;
	return String(n);
}

/** "Today", "Yesterday", or "November 13"; `today` is injectable for tests. */
export function fmtDate(tsMs: number, today: Date = new Date()): string {
	const d = new Date(tsMs);
	if (d.toDateString() === today.toDateString()) return "Today";
	const yesterday = new Date(today);
	yesterday.setDate(today.getDate() - 1);
	if (d.toDateString() === yesterday.toDateString()) return "Yesterday";
	return d.toLocaleDateString([], { month: "long", day: "numeric" });
}

/** True when a date separator belongs before task `i`: it starts a group and falls on another day than the one before. */
export function needsDateSeparator(tasks: Array<{ ts: number }>, groupStart: boolean, i: number): boolean {
	if (!groupStart || i <= 0) return false;
	return new Date(tasks[i].ts).toDateString() !== new Date(tasks[i - 1].ts).toDateString();
}

export function shouldCollapse(text: string): boolean {
	return text.split("\n").length > COLLAPSE_LINES;
}

/** The part of `text` shown while a long reply is folded. */
export function collapsedText(text: string): string {
	return text.split("\n").slice(0, COLLAPSE_LINES).join("\n");
}

/**
 * Format tool_input for approval bubble: Bash -> show command; Edit/Write -> file path;
 * else compact key: value lines.
 */
export function formatApprovalInput(toolName: string, toolInput: unknown): string {
	if (!toolInput || typeof toolInput !== "object") return String(toolInput ?? "");
	const inp = toolInput as Record<string, unknown>;
	if (toolName === "Bash" && typeof inp.command === "string") return inp.command;
	if ((toolName === "Write" || toolName === "Edit") && typeof inp.file_path === "string")
		return inp.file_path;
	return Object.entries(inp)
		.slice(0, 6)
		.map(([k, v]) => `${k}: ${typeof v === "string" ? v : JSON.stringify(v)}`)
		.join("\n");
}

/** Keys of the per-task "expanded" set: one set holds the folded/full/dismissed flags of every reply. */
export const expandKey = {
	full: (id: string) => `${id}:full`,
	dismissed: (id: string) => `${id}:dismissed`,
};

/** Immutable toggle for a `$state<Set>`: returns a new set with `key` added (on) or removed (off). */
export function withKey(set: Set<string>, key: string, on: boolean): Set<string> {
	const next = new Set(set);
	if (on) next.add(key);
	else next.delete(key);
	return next;
}
