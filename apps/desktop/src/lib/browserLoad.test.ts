// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { describeFailure, idle, step, type LoadEvent } from "./browserLoad.ts";

// Incident (2026-10-26): http://localhost:1 left a blank white rectangle. WKWebView reports a refused connection only
// as didFailProvisionalNavigation (no `load`, no wry page-load event), so the tab must treat `fail` as the signal.
const NS = "NSURLErrorDomain";
const fail = (code: number, domain = NS, url = "http://localhost:1/"): LoadEvent => ({
	event: "fail",
	url,
	code,
	domain,
	message: "x",
});
const run = (...events: LoadEvent[]) => events.reduce(step, idle);

test("nav starts loading, load ends it", () => {
	expect(run({ event: "nav", url: "http://a/" })).toEqual({ loading: true, failure: null, url: "http://a/" });
	expect(run({ event: "nav", url: "http://a/" }, { event: "load", url: "http://a/" })).toEqual(idle);
});

test("a refused connection fails the load with friendly text", () => {
	const s = run({ event: "nav", url: "http://localhost:1/" }, fail(-1004));
	expect(s.loading).toBe(false);
	expect(s.failure).toMatchObject({
		url: "http://localhost:1/",
		kind: "refused",
		title: "This page can't be reached",
	});
	expect(s.failure.hint).toContain("Is the dev server running");
});

test("every common NSURLError maps to a kind", () => {
	const kind = (code: number, domain = NS) => describeFailure(fail(code, domain)).kind;
	expect(kind(-1004)).toBe("refused"); // cannotConnectToHost
	expect(kind(-1003)).toBe("host"); // cannotFindHost
	expect(kind(-1200)).toBe("tls"); // secureConnectionFailed
	expect(kind(-1202)).toBe("tls"); // serverCertificateUntrusted
	expect(kind(-1205)).toBe("tls"); // clientCertificateRejected
	expect(kind(-1009)).toBe("offline"); // notConnectedToInternet
	expect(kind(-1001)).toBe("timeout"); // timedOut
	expect(kind(-1005)).toBe("other"); // networkConnectionLost
	expect(kind(1, "SomethingElse")).toBe("other");
});

test("a code from another domain is never read as a URL error", () => {
	expect(describeFailure(fail(-1004, "WebKitErrorDomain")).kind).toBe("other");
});

test("the unknown case still names the failure instead of hiding it", () => {
	const d = describeFailure({ ...fail(-1100), message: "The requested URL was not found" });
	expect(d.kind).toBe("other");
	expect(d.hint).toContain("The requested URL was not found");
});

test("a cancelled navigation (-999: superseded, redirected) is not a failure", () => {
	const s = run({ event: "nav", url: "http://a/" }, fail(-999, NS, "http://a/"));
	expect(s.failure).toBeNull();
	// still loading: the navigation that cancelled it is the one in flight
	expect(s.loading).toBe(true);
});

test("a navigation clears the previous failure; the new load ends clean", () => {
	const failed = run({ event: "nav", url: "http://a/" }, fail(-1004, NS, "http://a/"));
	expect(failed.failure).not.toBeNull();
	const s = [
		{ event: "nav", url: "http://b/" },
		{ event: "load", url: "http://b/" },
	].reduce(step, failed);
	expect(s).toEqual(idle);
});

test("reload clears the failure and shows loading before the page answers", () => {
	const failed = run(fail(-1004));
	expect(step(failed, { event: "reload" })).toMatchObject({ loading: true, failure: null });
});

test("a load that never answers fails as a timeout, once", () => {
	const s = run({ event: "nav", url: "http://slow/" }, { event: "stall", url: "http://slow/" });
	expect(s.loading).toBe(false);
	expect(s.failure.kind).toBe("timeout");
});

test("a stall after the page loaded is ignored", () => {
	const s = run(
		{ event: "nav", url: "http://a/" },
		{ event: "load", url: "http://a/" },
		{ event: "stall", url: "http://a/" },
	);
	expect(s).toEqual(idle);
});

test("a stall after a real failure keeps the real failure", () => {
	const s = run({ event: "nav", url: "http://a/" }, fail(-1004, NS, "http://a/"), {
		event: "stall",
		url: "http://a/",
	});
	expect(s.failure.kind).toBe("refused");
});

test("a late fail for a superseded url does not overwrite the current load", () => {
	const s = run(
		{ event: "nav", url: "http://a/" },
		{ event: "nav", url: "http://b/" },
		fail(-1004, NS, "http://a/"),
	);
	expect(s.failure).toBeNull();
	expect(s.loading).toBe(true);
});

test("events carry the real serialized payload shape of the native side", () => {
	// Exactly what `browser.rs` emits (serde_json), not a hand-written mock.
	const payload = JSON.parse(
		'{"event":"fail","url":"http://localhost:1/","code":-1004,"domain":"NSURLErrorDomain","message":"Could not connect to the server."}',
	);
	const s = run({ event: "nav", url: "http://localhost:1/" }, payload);
	expect(s.failure.kind).toBe("refused");
});
