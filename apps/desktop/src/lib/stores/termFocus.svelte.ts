/**
 * A GPU terminal's input sink has the keyboard focus. The native menu owns ⌘K (Jump); while this
 * is set the menu gives the key up (`menu.svelte.ts`), so it reaches the terminal, which clears
 * its screen like Ghostty's `clear_screen`. Everywhere else ⌘K stays the Jump palette.
 */
export const termFocus = $state({ on: false });
