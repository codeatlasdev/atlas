/**
 * The GPU terminals that are open, by tab id, so a command ("Reload terminal") can reach the active tab's native view
 * without the component handing itself around. The registry is the native-view core's (`native/surface.ts`): a
 * terminal's `NativeView` registers itself there under the `terminal` kind.
 */
import { hasNativeSurface, reloadNativeSurface } from "#lib/native/surface.ts";

/** Does `tab` have a native view to reload? (False for xterm.js tabs and non-terminals.) */
export const hasTermView = (tab: string | null | undefined) => hasNativeSurface("terminal", tab);

/** Reload `tab`'s native view; resolves `false` when it has none. */
export const reloadTermView = (tab: string | null | undefined) => reloadNativeSurface("terminal", tab);
