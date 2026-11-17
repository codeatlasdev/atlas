import { invoke } from "@tauri-apps/api/core";
import type { FileIo } from "#lib/editor/EditorSession.svelte.ts";

/**
 * The light editor's file access (#499): the app process reads and writes the one file itself (`loose_fs.rs`), so a
 * loose file opens without waiting for hostd, which a cold start may still be spawning. Nothing watches the file:
 * the session still compares the disk with what it last read before every save, so an outside change is never
 * overwritten, it just is not shown until then.
 */
export const looseIo: FileIo = {
	fsStat: (path) => invoke("loose_stat", { path }),
	fsRead: (path, max) => invoke("loose_read", { path, max }),
	fsWrite: (path, text) => invoke("loose_write", { path, text }),
	fsWatch: async () => undefined,
	fsUnwatch: async () => undefined,
};
