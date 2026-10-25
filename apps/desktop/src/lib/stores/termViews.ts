/**
 * The native views of the GPU terminals that are open, by tab id, so a command ("Reload terminal")
 * can reach the active tab's view without the component handing itself around.
 */
export type Reloadable = { reload(): Promise<void> };

const views = new Map<string, Reloadable>();

/** Register `view` for `tab`; returns the unregister function (call it on unmount). */
export function registerTermView(tab: string, view: Reloadable): () => void {
	views.set(tab, view);
	return () => {
		// A remounted tab may already have registered its new view: only remove our own.
		if (views.get(tab) === view) views.delete(tab);
	};
}

/** Does `tab` have a native view to reload? (False for xterm.js tabs and non-terminals.) */
export function hasTermView(tab: string | null | undefined): boolean {
	return !!tab && views.has(tab);
}

/** Reload `tab`'s native view; resolves `false` when it has none. */
export async function reloadTermView(tab: string | null | undefined): Promise<boolean> {
	const v = tab ? views.get(tab) : undefined;
	if (!v) return false;
	await v.reload();
	return true;
}
