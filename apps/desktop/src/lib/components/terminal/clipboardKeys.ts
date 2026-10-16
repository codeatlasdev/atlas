/** Clipboard keys and clipboard writes of the xterm.js terminal (Linux; macOS uses the native Edit menu). */

type Key = Pick<KeyboardEvent, "type" | "key" | "ctrlKey" | "shiftKey" | "altKey" | "metaKey">;

/**
 * What a key does to the clipboard, by Linux terminal convention (GNOME Terminal, Konsole, Kitty, VS Code):
 * `copy` / `paste` = Ctrl+Shift+C / Ctrl+Shift+V; Ctrl+C also copies while text is selected and otherwise is
 * the program's SIGINT (`null` = not ours, leave it to xterm). `swallow` = Ctrl+Shift+C with nothing selected:
 * the chord is ours and must not reach the program as `^C`.
 * On macOS ⌘C/⌘V belong to the native Edit menu and Ctrl+C is always SIGINT: always `null`.
 */
export function keyAction(
	e: Key,
	ctx: { mac: boolean; hasSelection: boolean },
): "copy" | "paste" | "swallow" | null {
	if (ctx.mac || e.type !== "keydown" || !e.ctrlKey || e.altKey || e.metaKey) return null;
	const k = e.key.toLowerCase();
	if (e.shiftKey) {
		if (k === "c") return ctx.hasSelection ? "copy" : "swallow";
		return k === "v" ? "paste" : null;
	}
	return k === "c" && ctx.hasSelection ? "copy" : null;
}

/**
 * Text → the system clipboard. `navigator.clipboard` is missing or rejected in some WebKitGTK builds (secure
 * context, user-gesture state), so a rejection falls back to `execCommand("copy")` on a scratch textarea,
 * which works inside a key or mouse handler. `false` = nothing was copied.
 */
export async function copyText(text: string): Promise<boolean> {
	if (!text) return false;
	try {
		await navigator.clipboard.writeText(text);
		return true;
	} catch {
		return execCopy(text);
	}
}

function execCopy(text: string): boolean {
	const active = document.activeElement as HTMLElement | null;
	const ta = document.createElement("textarea");
	ta.value = text;
	ta.setAttribute("readonly", "");
	ta.style.cssText = "position:fixed;top:0;left:0;opacity:0;pointer-events:none";
	document.body.append(ta);
	try {
		ta.select();
		return document.execCommand("copy");
	} catch {
		return false;
	} finally {
		ta.remove();
		active?.focus?.();
	}
}
