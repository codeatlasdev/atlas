/**
 * AI commit message: the events `commit_msg_generate` streams (`crates/atlas-desktop/src/commit_msg.rs`, whose
 * serialization is the fixture the tests read) and the state the commit box derives from them. Pure: no Svelte, no IPC.
 */
export interface DigestInfo {
	summarized: number;
	withheld: number;
	truncated: number;
	omitted: number;
}
export type CommitEvent =
	| { kind: "start"; agent: string; scope: "staged" | "all"; digest: DigestInfo }
	| { kind: "chunk"; text: string }
	| { kind: "done"; message: string }
	| { kind: "error"; message: string }
	| { kind: "cancelled" };

export interface CommitRun {
	phase: "idle" | "running";
	request: string;
	agent: string;
	scope: "staged" | "all";
	/** The message so far (whole text, not a delta). */
	text: string;
	/** What was not sent in full, or that all changes were used; empty when there is nothing to say. */
	notice: string;
	error: string;
	cancelled: boolean;
}

const isObj = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null;

/** Validate one event of the Rust payload; a shape drift throws instead of leaving the spinner running forever. */
export function parseEvent(v: unknown): CommitEvent {
	if (isObj(v)) {
		const str = (k: string) => (typeof v[k] === "string" ? (v[k] as string) : null);
		switch (v.kind) {
			case "start": {
				const d = v.digest;
				const n = (k: string) => (isObj(d) && typeof d[k] === "number" ? (d[k] as number) : 0);
				const agent = str("agent");
				if (agent !== null && (v.scope === "staged" || v.scope === "all"))
					return {
						kind: "start",
						agent,
						scope: v.scope,
						digest: {
							summarized: n("summarized"),
							withheld: n("withheld"),
							truncated: n("truncated"),
							omitted: n("omitted"),
						},
					};
				break;
			}
			case "chunk":
				if (str("text") !== null) return { kind: "chunk", text: str("text")! };
				break;
			case "done":
				if (str("message") !== null) return { kind: "done", message: str("message")! };
				break;
			case "error":
				if (str("message") !== null) return { kind: "error", message: str("message")! };
				break;
			case "cancelled":
				return { kind: "cancelled" };
		}
	}
	throw new Error(`commit-msg: unexpected event ${JSON.stringify(v)}`);
}

export function idle(): CommitRun {
	return {
		phase: "idle",
		request: "",
		agent: "",
		scope: "staged",
		text: "",
		notice: "",
		error: "",
		cancelled: false,
	};
}

export function startState(request: string): CommitRun {
	return { ...idle(), phase: "running", request };
}

/** One line on what the provider did not get in full ("2 files withheld (secrets), 3 cut"). */
function noticeOf(scope: "staged" | "all", d: DigestInfo): string {
	const parts: string[] = [];
	if (d.withheld) parts.push(`${d.withheld} file${d.withheld > 1 ? "s" : ""} withheld (may hold secrets)`);
	if (d.truncated) parts.push(`${d.truncated} cut`);
	if (d.omitted) parts.push(`${d.omitted} left out (too large)`);
	if (d.summarized) parts.push(`${d.summarized} named only`);
	const what = parts.join(", ");
	return scope === "all" ? `Nothing was staged: used all changes${what ? `. ${what}` : ""}` : what;
}

/** Fold one event into the run. Events of another request (`from` ≠ this run) are not ours. */
export function applyEvent(s: CommitRun, e: CommitEvent, from = s.request): CommitRun {
	if (from !== s.request) return s;
	switch (e.kind) {
		case "start":
			return { ...s, agent: e.agent, scope: e.scope, notice: noticeOf(e.scope, e.digest) };
		case "chunk":
			return { ...s, text: e.text };
		case "done":
			return { ...s, phase: "idle", text: e.message };
		case "error":
			return { ...s, phase: "idle", error: e.message };
		case "cancelled":
			return { ...s, phase: "idle", cancelled: true };
	}
}

export interface ProviderState {
	state: "ready" | "off" | "none";
	agent?: string;
}

/** `show`: render the button; `setup`: clicking explains how to pick a provider instead of generating. */
export function providerHint(p: ProviderState): { show: boolean; setup: boolean } {
	return { show: p.state !== "off", setup: p.state === "none" };
}
