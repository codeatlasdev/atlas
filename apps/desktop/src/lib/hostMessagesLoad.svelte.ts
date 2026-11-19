/**
 * Loading of `#lib/hostMessages.ts` (the catalog side of `#lib/hostText.ts`), on demand and never at startup: the
 * 17 messages are only needed when a host failure, toast or status reason is shown, and a static import would put
 * them in the startup chunk (apps/desktop/bundle-budget.json). `loaded()` is reactive: an effect or `$derived` that
 * read it re-runs when the load settles.
 */
import type * as HostMessages from "#lib/hostMessages.ts";

type Messages = Pick<typeof HostMessages, "render"> & Partial<Pick<typeof HostMessages, "hostError">>;

let impl = $state<Messages>();
let loading: Promise<void> | undefined;

/** The messages, `undefined` until they load. A chunk that cannot be loaded settles as messages that know no code. */
export function loaded(): Messages | undefined {
	return impl;
}

/** Load the messages (once, shared by concurrent callers); never rejects. */
export function loadHostMessages(): Promise<void> {
	loading ??= import("#lib/hostMessages.ts").then(
		(mod) => void (impl = mod),
		() => void (impl = { render: () => undefined }),
	);
	return loading;
}

/** For tests: forget the loaded messages, as at startup. */
export function unloadHostMessages() {
	impl = undefined;
	loading = undefined;
}
