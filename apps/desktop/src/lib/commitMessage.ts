/**
 * AI commit message: the events `commit_msg_generate` streams (`crates/atlas-desktop/src/commit_msg.rs`, whose
 * serialization is the fixture the tests read) and the state the commit box derives from them. Pure: no Svelte, no IPC.
 */
import { list } from "#lib/i18n/fmt.ts";
import { m } from "#lib/i18n/m.ts";

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
	/** What was not sent in full; null before the run starts. The sentence is built when it is shown (`commitNotice`). */
	digest: DigestInfo | null;
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
		digest: null,
		error: "",
		cancelled: false,
	};
}

export function startState(request: string): CommitRun {
	return { ...idle(), phase: "running", request };
}

/** One line on what the provider did not get in full ("2 files withheld (secrets), 3 cut"); empty when it got everything. */
export function commitNotice(s: Pick<CommitRun, "scope" | "digest">): string {
	const d = s.digest;
	if (!d) return "";
	const parts: string[] = [];
	if (d.withheld) parts.push(m.git_commit_notice_withheld({ count: d.withheld }));
	if (d.truncated) parts.push(m.git_commit_notice_cut({ count: d.truncated }));
	if (d.omitted) parts.push(m.git_commit_notice_left_out({ count: d.omitted }));
	if (d.summarized) parts.push(m.git_commit_notice_named_only({ count: d.summarized }));
	const omitted = list(parts, "unit");
	if (s.scope === "all")
		return omitted ? m.git_commit_notice_all_with({ omitted }) : m.git_commit_notice_all();
	return omitted;
}

/**
 * A failure of `commit_msg_generate` or of the provider run, in words. The Rust side sends a stable code for the
 * failures it knows (the contract is `__fixtures__/commit-msg-errors.json`); anything else is the agent's own output
 * and shows as it came.
 */
export function commitErrorText(raw: string): string {
	switch (raw) {
		case "commit_msg.ssh_host":
			return m.git_commit_error_ssh();
		case "commit_msg.provider_off":
			return m.git_commit_error_off();
		case "commit_msg.no_changes":
			return m.git_commit_error_no_changes();
		case "commit_msg.all_secret":
			return m.git_commit_error_all_secret();
		case "commit_msg.empty_message":
			return m.git_commit_error_empty();
		default:
			return raw;
	}
}

/** Fold one event into the run. Events of another request (`from` ≠ this run) are not ours. */
export function applyEvent(s: CommitRun, e: CommitEvent, from = s.request): CommitRun {
	if (from !== s.request) return s;
	switch (e.kind) {
		case "start":
			return { ...s, agent: e.agent, scope: e.scope, digest: e.digest };
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
