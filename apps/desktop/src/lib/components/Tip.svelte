<script lang="ts">
	import { Tooltip } from "@codeatlasdev/ui";
	import type { Snippet } from "svelte";
	import { shortcutFor } from "#lib/keymap.svelte.ts";

	/**
	 * Tooltip for an interactive control; appends the command's current key binding
	 * (it follows user remaps), e.g. "Toggle left dock  ⌘B".
	 */
	let {
		label,
		command,
		side = "bottom",
		child,
	}: {
		label: string;
		/** Command id whose binding is shown next to the label. */
		command?: string;
		side?: "top" | "bottom" | "left" | "right";
		/** The control; spread `props` onto it. */
		child: Snippet<[Record<string, unknown>]>;
	} = $props();

	let keys = $derived(command ? shortcutFor(command) : undefined);
</script>

<Tooltip content={keys ? `${label}  ${keys}` : label} {side} delay={400} triggerChild={child} />
