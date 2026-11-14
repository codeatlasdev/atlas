/**
 * Owns: reading an ask card out of a task's `result` text. A crewmate that needs a choice replies with JSON
 * `{"ask": {...}}`; anything else is a normal markdown reply.
 */

export interface AskPayload {
	id: string;
	question: string;
	options: Array<{ key: string; title: string; subtitle?: string }>;
	allow_text?: boolean;
}

/** The ask card in `result`, or null when it is a plain reply (or not valid JSON). */
export function parseAsk(result?: string): AskPayload | null {
	if (!result?.trimStart().startsWith("{")) return null;
	try {
		const v = JSON.parse(result);
		if (v && typeof v === "object" && "ask" in v) return v.ask as AskPayload;
	} catch {
		/* not JSON */
	}
	return null;
}
