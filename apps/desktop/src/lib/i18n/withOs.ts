/**
 * The `os` input of a message that names an OS noun ("your Mac", Finder / File Explorer, Trash / Recycle Bin): the
 * catalog selects on it (`"os=mac"`, `"os=windows"`, `"os=*"`) and no call site passes it. Paraglide has no global
 * inputs, so `scripts/paraglide.ts` writes `messages.gen.ts`, which wraps each message of the catalog that has an `os`
 * input with `withOs` (see `m.ts`). A caller may still pass `os` to say another one (tests do).
 */
import type { Os } from "#lib/keys.ts";

type Inputs<F> = F extends (inputs: infer I, ...rest: never[]) => unknown
	? Omit<NonNullable<I>, "os">
	: never;

/** A generated message whose `os` input became optional (and absent when the message does not use it). */
export type WithOs<F> = F extends (inputs: never, options?: infer O) => infer R
	? {} extends Inputs<F>
		? (inputs?: Inputs<F> & { os?: Os }, options?: O) => R
		: (inputs: Inputs<F> & { os?: Os }, options?: O) => R
	: F;

/**
 * `message` with `os` (the current one by default) added to its inputs. Marked as free of side effects: the generated
 * module calls it once per message at load, and the bundler drops the calls of the messages nobody uses.
 */
/* @__NO_SIDE_EFFECTS__ */
export function withOs<F extends (inputs: never, options?: never) => unknown>(
	message: F,
	os: () => Os,
): WithOs<F> {
	const wrapped = (inputs?: object, options?: object) =>
		(message as Function)({ os: os(), ...inputs }, options);
	return wrapped as WithOs<F>;
}
