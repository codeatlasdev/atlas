/** Palette commands for themes (`Theme: <name>`, next/previous, import, Ghostty) and the one-time Ghostty offer. */
import { toast } from "@codeatlasdev/ui";
import { named, register, type Command } from "#lib/keymap.svelte.ts";
import { m } from "#lib/i18n/m.ts";
import { themes } from "#lib/stores/theme.svelte.ts";
import { updates } from "#lib/stores/updates.svelte.ts";
import type { Theme } from "#lib/theme.ts";

/** Select `t` for its own appearance; the other appearance's slot does not change what is on screen. */
export function useTheme(t: Theme) {
	const showing = themes.current?.appearance;
	void themes.setTheme({ [t.appearance]: t.name });
	if (showing && showing !== t.appearance)
		toast.info(
			t.appearance === "dark"
				? m.shell_commands_theme_used_in_dark()
				: m.shell_commands_theme_used_in_light(),
		);
}

export function themeCommands(): Command[] {
	return named([
		...themes.list.map((t) => ({
			id: `theme::Set:${t.name}`,
			label: () => m.shell_commands_theme_named({ name: t.name }),
			run: () => useTheme(t),
		})),
		{ id: "theme::Next", label: () => m.shell_commands_theme_next(), run: () => themes.cycle(1) },
		{ id: "theme::Previous", label: () => m.shell_commands_theme_previous(), run: () => themes.cycle(-1) },
		{
			id: "theme::Import",
			label: () => m.shell_commands_theme_import(),
			run: () => updates.show("appearance"),
		},
		{
			id: "theme::UseGhostty",
			label: () => m.shell_commands_theme_use_ghostty(),
			run: () => updates.show("appearance"),
			when: () => themes.ghostty.available,
		},
		{
			id: "terminal-theme::App",
			label: () => m.shell_commands_terminal_theme_same_as_app(),
			run: () => void themes.setTheme({ terminal: "app" }),
		},
		...themes.list.map((t) => ({
			id: `terminal-theme::Set:${t.name}`,
			label: () => m.shell_commands_terminal_theme_named({ name: t.name }),
			run: () => void themes.setTheme({ terminal: t.name }),
		})),
	]);
}

/**
 * Keeps the registry in step with `themes.list` (user themes appear and disappear as files change).
 * Call inside a reactive root; returns the disposer.
 */
export function startThemeCommands(): () => void {
	let unregister = () => {};
	const stop = $effect.root(() => {
		$effect(() => {
			const off = register(themeCommands());
			unregister();
			unregister = off;
		});
	});
	return () => {
		stop();
		unregister();
	};
}

/** Drives `GhosttyOffer.svelte`: set once, after `themes.init()`, while the offer is still live. */
export const ghosttyOffer = $state({ shown: false });

/** Offer the Ghostty theme once per session; it never applies anything by itself. */
export function offerGhostty() {
	if (themes.ghostty.offered) ghosttyOffer.shown = true;
}

/** The toast's `Use theme` button: open Settings > Appearance, where the banner applies it. */
export function openGhosttyOffer() {
	ghosttyOffer.shown = false;
	updates.show("appearance");
}
