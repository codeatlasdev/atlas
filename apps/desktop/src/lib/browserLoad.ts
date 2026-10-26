/**
 * Load state of one embedded browser tab, driven by the `browser://<tab>` events of the native side
 * (`crates/atlas-desktop/src/browser.rs`). Pure: the component feeds events in, draws `state`.
 *
 * WKWebView reports a refused / unresolvable / untrusted load only as `didFailProvisionalNavigation`: wry emits no
 * page-load event for it, so `fail` (native) is the one real signal. `stall` is the frontend's own timer for a load
 * that neither finishes nor fails.
 */

export type LoadEvent =
	| { event: "nav"; url?: string }
	| { event: "load"; url?: string }
	| { event: "fail"; url?: string; code: number; domain: string; message: string }
	| { event: "stall"; url?: string }
	| { event: "reload" };

export type FailureKind = "refused" | "host" | "tls" | "offline" | "timeout" | "other";

export interface Failure {
	url: string;
	kind: FailureKind;
	title: string;
	hint: string;
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

const HINTS: Record<Exclude<FailureKind, "other">, string> = {
	refused: "Nothing answered at this address. Is the dev server running?",
	host: "The server's address could not be found. Check it for typos.",
	tls: "A secure connection could not be made: the certificate is missing, expired or not trusted.",
	offline: "This computer is not connected to the internet.",
	timeout: "The server took too long to answer.",
};

export function describeFailure(e: { url?: string; code: number; domain: string; message: string }): Failure {
	const kind = e.domain === URL_ERROR_DOMAIN ? (KINDS[e.code] ?? "other") : "other";
	return {
		url: e.url ?? "",
		kind,
		title: "This page can't be reached",
		hint: kind === "other" ? e.message || "The page could not be loaded." : HINTS[kind],
	};
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
