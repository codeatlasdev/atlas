import { isHostFailure } from "#lib/hostText.ts";
import { messageOf } from "#lib/errors/report.ts";
import type { FsEntry } from "#lib/host.ts";

/** What the explorer never lists. */
const HIDDEN: Record<string, true> = { ".git": true, ".DS_Store": true };

type LoadResult = { ok: true; entries: FsEntry[] } | { ok: false; retry: boolean };

/**
 * List one folder. A terminal-host outage goes to `onHost` (one shared toast, not one per folder) and the folder
 * stays retryable; every other error is the folder's own (`onOther`).
 */
export async function loadFolder(
	list: () => Promise<FsEntry[]>,
	on: { onHost: (message: string) => void; onOther: (message: string) => void },
): Promise<LoadResult> {
	try {
		return { ok: true, entries: (await list()).filter((e) => !HIDDEN[e.name]) };
	} catch (e) {
		const m = messageOf(e);
		if (isHostFailure(e, m)) {
			on.onHost(m);
			return { ok: false, retry: true };
		}
		on.onOther(m);
		return { ok: false, retry: false };
	}
}

/** Folders to list again once the host is back: the root, every expanded folder and those that failed meanwhile. */
export function foldersToReload(
	expanded: Record<string, boolean>,
	pending: Set<string> = new Set(),
): string[] {
	const out = new Set<string>([""]);
	for (const [rel, open] of Object.entries(expanded)) if (open) out.add(rel);
	for (const rel of pending) out.add(rel);
	return [...out];
}
