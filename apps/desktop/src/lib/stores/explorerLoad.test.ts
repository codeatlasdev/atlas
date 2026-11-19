// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { hostRejection } from "#lib/hostText.ts";
import { loadFolder, foldersToReload } from "./explorerLoad.ts";

const e = (name, kind = "file") => ({ name, kind, size: 0, mtime: 0 });
const HOST_DOWN = "Terminal host not reachable. Terminals cannot start or reconnect until it is back.";

/**
 * Incident 1.5.0-alpha.5: with the terminal host unreachable every folder load toasted its own
 * error and the tree stayed empty ("the Files panel does not open"), and nothing reloaded it when
 * the host came back. A host outage is reported once (by the host toast) and leaves the folder
 * retryable; any other error (a folder that vanished) is still the folder's own toast.
 */
test("a host outage is not a per-folder error and leaves the folder to retry", async () => {
	const seen = { host: [], other: [] };
	const r = await loadFolder(
		async () => {
			throw HOST_DOWN;
		},
		{ onHost: (m) => seen.host.push(m), onOther: (m) => seen.other.push(m) },
	);
	expect(r).toEqual({ ok: false, retry: true });
	expect(seen).toEqual({ host: [HOST_DOWN], other: [] });
});

test("any other failure is the folder's own error and is not retried blindly", async () => {
	const seen = { host: [], other: [] };
	const r = await loadFolder(
		async () => {
			throw new Error("No such file or directory");
		},
		{ onHost: (m) => seen.host.push(m), onOther: (m) => seen.other.push(m) },
	);
	expect(r).toEqual({ ok: false, retry: false });
	expect(seen).toEqual({ host: [], other: ["No such file or directory"] });
});

test("a good listing hides what the explorer hides and reports ok", async () => {
	const r = await loadFolder(async () => [e(".git", "dir"), e("a.ts"), e(".DS_Store")], {
		onHost() {},
		onOther() {},
	});
	expect(r.ok).toBe(true);
	expect(r.entries.map((x) => x.name)).toEqual(["a.ts"]);
});

test("after a reconnect, the root and every expanded folder reload; collapsed ones stay lazy", () => {
	expect(foldersToReload({ "": true, src: true, lib: false, "src/ui": true })).toEqual(["", "src", "src/ui"]);
	// A folder that failed to load while the host was down is reloaded even though it has no entries yet.
	expect(foldersToReload({ "": true }, new Set(["pending"]))).toEqual(["", "pending"]);
});

// Regression (R10 review): a remote crew server still connecting rejected with the coded `hostd.remote_connecting`,
// which `isHostFailure` took for an outage of the local host (the one toast with "Restart terminal host").
test("a remote host that is still connecting is the folder's own error, not a local outage", async () => {
	const seen = { host: [], other: [] };
	const r = await loadFolder(
		async () => {
			throw await hostRejection({
				code: "hostd.remote_connecting",
				args: { host: "ssh:box" },
				message: "ssh:box is still connecting; try again in a few seconds",
			});
		},
		{ onHost: (m) => seen.host.push(m), onOther: (m) => seen.other.push(m) },
	);
	expect(r).toEqual({ ok: false, retry: false });
	expect(seen.host).toEqual([]);
	expect(seen.other).toEqual(["ssh:box is still connecting; try again in a few seconds"]);
});
