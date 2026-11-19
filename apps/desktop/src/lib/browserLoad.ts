/**
 * Load state of one embedded browser tab, driven by the `browser://<tab>` events of the native side
 * (`crates/atlas-desktop/src/browser.rs`). Pure: the component feeds events in, draws `state`.
 *
 * WKWebView reports a refused / unresolvable / untrusted load only as `didFailProvisionalNavigation`: wry emits no
 * page-load event for it, so `fail` (native) is the one real signal. `stall` is the frontend's own timer for a load
 * that neither finishes nor fails.
 */
import { m } from "#lib/i18n/m.ts";

export type LoadEvent =
	| { event: "nav"; url?: string }
	| { event: "load"; url?: string }
	| { event: "fail"; url?: string; code: number; domain: string; message: string }
	| { event: "stall"; url?: string }
	| { event: "reload" };

export type FailureKind = "refused" | "host" | "tls" | "offline" | "timeout" | "other";

/** What failed, not how it reads: the words are made at display time (`failureHint`), so a language switch reaches a screen that is already showing. */
export interface Failure {
	url: string;
	kind: FailureKind;
	/** The engine's own wording, shown only for `other` (nothing in the catalog can say what it did not name). */
	message: string;
}

export interface LoadState {
	loading: boolean;
	failure: Failure | null;
	/** URL of the navigation in flight: a failure of an older one (superseded) is dropped. */
	url?: string;
}

export const idle: LoadState = { loading: false, failure: null };

const URL_ERROR_DOMAIN = "NSURLErrorDomain";
/** `NSURLErrorCancelled`: the navigation was superseded (a redirect, a newer load); never an error to show. */
const CANCELLED = -999;

/** NSURLError codes (Foundation `NSURLError.h`) by what the user can do about them. */
const KINDS: Record<number, FailureKind> = {
	[-1004]: "refused", // cannotConnectToHost
	[-1003]: "host", // cannotFindHost
	[-1006]: "host", // dnsLookupFailed
	[-1009]: "offline", // notConnectedToInternet
	[-1001]: "timeout", // timedOut
	[-1200]: "tls", // secureConnectionFailed
	[-1201]: "tls", // serverCertificateHasBadDate
	[-1202]: "tls", // serverCertificateUntrusted
	[-1203]: "tls", // serverCertificateHasUnknownRoot
	[-1204]: "tls", // serverCertificateNotYetValid
	[-1205]: "tls", // clientCertificateRejected
	[-1206]: "tls", // clientCertificateRequired
};

const HINTS: Record<Exclude<FailureKind, "other">, () => string> = {
	refused: () => m.workbench_browser_failure_refused(),
	host: () => m.workbench_browser_failure_host(),
	tls: () => m.workbench_browser_failure_tls(),
	offline: () => m.workbench_browser_failure_offline(),
	timeout: () => m.workbench_browser_failure_timeout(),
};

export function describeFailure(e: { url?: string; code: number; domain: string; message: string }): Failure {
	const kind = e.domain === URL_ERROR_DOMAIN ? (KINDS[e.code] ?? "other") : "other";
	return { url: e.url ?? "", kind, message: e.message };
}

/** One line on what the person can do about it; an unnamed failure shows the engine's words, or says it did not load. */
export function failureHint(f: Failure): string {
	return f.kind === "other" ? f.message || m.workbench_browser_failure_other() : HINTS[f.kind]();
}

export function step(s: LoadState, e: LoadEvent): LoadState {
	switch (e.event) {
		case "nav":
			return { loading: true, failure: null, url: e.url };
		case "reload":
			return { loading: true, failure: null, url: s.failure?.url ?? s.url };
		case "load":
			return idle;
		case "stall":
			// Only a load still waiting can stall; a finished or already failed one keeps what it has.
			return s.loading
				? {
						loading: false,
						failure: describeFailure({ url: e.url, code: -1001, domain: URL_ERROR_DOMAIN, message: "" }),
						url: s.url,
					}
				: s;
		case "fail":
			if (e.code === CANCELLED && e.domain === URL_ERROR_DOMAIN) return s;
			if (s.url && e.url && e.url !== s.url) return s;
			return { loading: false, failure: describeFailure(e), url: s.url ?? e.url };
	}
}
