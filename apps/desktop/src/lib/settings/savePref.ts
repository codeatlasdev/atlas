import { toast } from "@codeatlasdev/ui";
import { errText } from "#lib/api.ts";
import type { Prefs } from "#lib/prefs.ts";
import { setPref } from "#lib/stores/prefs.svelte.ts";

/** Persist one top-level preference; failures surface as a toast. */
export function savePref<K extends keyof Prefs>(key: K, value: Prefs[K]) {
	setPref(key, value).catch((e) => toast.error(errText(e)));
}
