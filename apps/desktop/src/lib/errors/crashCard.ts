/**
 * The "closed unexpectedly last time" card (#322), page side. `crates/atlas-desktop/src/crash_card.rs` decides which
 * earlier crash is news (newer than the stamp the user dismissed) and hands over `{ atMs, kind, summary }`, already
 * redacted; this reads it defensively and turns it into the `ErrorReport` the existing report screen files.
 */
import { m } from "#lib/i18n/m.ts";
import { headline, redactText, type ErrorReport } from "./report.ts";

export interface CrashCard {
	/** Unix ms of the event; what a dismiss acknowledges. */
	atMs: number;
	/** `crash`: a crash report file; `hostd-exited`: the terminal host ended on its own. */
	kind: "crash" | "hostd-exited";
	summary: string;
	/** What names this crash across reports (`atlas_core::crash_fingerprint`); null for a host exit or an older build. */
	fingerprint: string | null;
}

/** The card out of whatever `crash_card_pending` returned; anything malformed is no card. */
export function parseCard(raw: unknown): CrashCard | null {
	if (!raw || typeof raw !== "object") return null;
	const r = raw as Record<string, unknown>;
	const kind = r.kind === "crash" || r.kind === "hostd-exited" ? r.kind : null;
	if (!kind || typeof r.atMs !== "number" || !Number.isFinite(r.atMs) || typeof r.summary !== "string")
		return null;
	const fingerprint =
		typeof r.fingerprint === "string" && /^[0-9a-f]{16}$/.test(r.fingerprint) ? r.fingerprint : null;
	return { atMs: r.atMs, kind, summary: r.summary, fingerprint };
}

/** The card's title and one line of body. The title is the report screen's own headline, so the two never drift. */
export function copy(card: CrashCard): { title: string; body: string } {
	const { title } = headline(crashReport(card));
	return {
		title,
		body: card.kind === "crash" ? m.errors_crash_card_body_crash() : m.errors_crash_card_body_hostd_exited(),
	};
}

/** The report "Report" opens: the redacted summary of the earlier event, filed through the same screen as any error. */
export function crashReport(card: CrashCard): ErrorReport {
	return {
		message: redactText(card.summary),
		stack: "",
		where: "earlier session",
		kind: card.kind,
		...(card.fingerprint ? { fingerprint: card.fingerprint } : {}),
		headline:
			card.kind === "crash"
				? { title: m.errors_crash_card_title_crash(), hint: m.errors_crash_card_hint_crash() }
				: { title: m.errors_crash_card_title_hostd_exited(), hint: m.errors_crash_card_hint_hostd_exited() },
	};
}
