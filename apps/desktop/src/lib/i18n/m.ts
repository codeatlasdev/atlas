/**
 * `m`: every UI message. Import it from here, never from `#lib/paraglide/messages.js`.
 *
 * Two jobs on top of the generated functions, in this one place:
 *  - it registers the Svelte-state locale strategy (importing `locale.svelte.ts`), so no message can render before the
 *    language is wired (Paraglide would fall back to English without a word);
 *  - it adds the `os` input (`mac` | `windows` | `linux`) to every call. A message that names an OS noun ("your Mac",
 *    "this PC", Finder / File Explorer, Trash / Recycle Bin, Option / Alt) selects on it in the catalog
 *    (`"os=mac"`, `"os=windows"`, `"os=*"`), and no call site passes it. Paraglide has no global inputs, hence the
 *    wrapper. A caller may still pass `os` to say another one (tests do).
 *
 * Shortcuts never live in a catalog text: a message takes a placeholder (`{key}`) and the caller fills it with `kbd()` /
 * `label()` from `keys.ts`, which follow the OS and the user's remaps.
 */
import * as generated from "#lib/paraglide/messages.js";
import "./locale.svelte.ts";
// A cycle with keys.ts (it builds text through `m`) that is safe: `OS` is only read when a message is called.
import { OS, type Os } from "#lib/keys.ts";

type Inputs<F> = F extends (inputs: infer I, ...rest: never[]) => unknown
	? Omit<NonNullable<I>, "os">
	: never;

/** A generated message whose `os` input became optional (and absent when the message does not use it). */
export type WithOs<F> = F extends (inputs: never, options?: infer O) => infer R
	? {} extends Inputs<F>
		? (inputs?: Inputs<F> & { os?: Os }, options?: O) => R
		: (inputs: Inputs<F> & { os?: Os }, options?: O) => R
	: F;

export type Messages<M> = { [K in keyof M]: WithOs<M[K]> };

/** `messages` with `os` (the current one by default) added to the inputs of every call. */
export function withOs<M extends object>(messages: M, os: () => Os): Messages<M> {
	const wrapped = new Map<string | symbol, unknown>();
	return new Proxy({} as Messages<M>, {
		get(_, key) {
			let f = wrapped.get(key);
			if (f === undefined) {
				const message = (messages as Record<string | symbol, unknown>)[key];
				if (typeof message !== "function") return message;
				f = (inputs?: object, options?: object) => message({ os: os(), ...inputs }, options);
				wrapped.set(key, f);
			}
			return f;
		},
	});
}

export const m = withOs(generated.m, () => OS);
