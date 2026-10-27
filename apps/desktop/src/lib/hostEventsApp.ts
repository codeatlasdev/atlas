import { listen } from "@tauri-apps/api/event";
import { createHostEvents } from "#lib/hostEvents.ts";

/** The page's one dispatcher for hostd's `host://event` and `host://reconnected` (see `hostEvents.ts`). */
export const hostEvents = createHostEvents((name, cb) => listen(name, cb));
