/** The capture history, from Rust (`shots_history`: already pruned of files that no longer exist). */
import { invoke } from "@tauri-apps/api/core";
import type { Entry, History } from "./types.ts";

/** Newest first, whatever order Rust sent (the sort is stable, so equal times keep Rust's order). */
export async function loadHistory(): Promise<Entry[]> {
	const h = await invoke<History>("shots_history");
	return [...(h.entries ?? [])].sort((a, b) => b.at - a.at);
}

/** Forget an entry. The file stays on disk. */
export const remove = (path: string) => invoke<void>("shots_history_remove", { path });

export const reveal = (path: string) => invoke<void>("shots_history_reveal", { path });

export const open = (path: string) => invoke<void>("shots_history_open", { path });
