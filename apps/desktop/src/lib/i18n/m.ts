/**
 * `m`: every UI message. Import it from here, never from `#lib/paraglide/messages.js`.
 *
 * Two jobs on top of the generated functions, in this one place:
 *  - it registers the Svelte-state locale strategy (importing `locale.svelte.ts`), so no message can render before the
 *    language is wired (Paraglide would fall back to English without a word);
 *  - it adds the `os` input (`mac` | `windows` | `linux`) to every message that has one (`withOs.ts`): the catalog
 *    selects on it, and no call site passes it. `messages.gen.ts` (written by `scripts/paraglide.ts`, gitignored) does
 *    that per message.
 *
 * Messages tree-shake: `m` is a namespace, and the bundler keeps the messages a component reaches through a static
 * `m.key` and nothing else, so a catalog of thousands of keys costs the chunk of the screen that shows them, not
 * the startup. Keep it that way: never `m[someVariable]`, never `Object.keys(m)`, never pass `m` itself around; a
 * dynamic key gets a `switch` over static `m.a() / m.b()` calls.
 *
 * Shortcuts never live in a catalog text: a message takes a placeholder (`{key}`) and the caller fills it with `kbd()` /
 * `label()` from `keys.ts`, which follow the OS and the user's remaps.
 */
import "./locale.svelte.ts";

export * as m from "./messages.gen.ts";
