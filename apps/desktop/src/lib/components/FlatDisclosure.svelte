<script lang="ts">
	/**
	 * The kit's `Collapsible` without its box. Inside a Settings card (which is already a surface) the kit draws a
	 * bordered glass bar for the trigger and a second bordered box for the body: box in a box, with the paddings
	 * added up and an edge that lines up with nothing. This is the one place that flattens it: a hairline-free text
	 * trigger and an unframed body, aligned with the row it sits in.
	 *
	 * Workaround for codeatlasdev/ui#912 (a `variant="flush"` Collapsible; fix in ui PR #964, not in 4.6.0). When the
	 * kit ships it, pass the variant here and delete the overrides below; no caller changes.
	 */
	import { Collapsible } from "@codeatlasdev/ui";
	import type { ComponentProps } from "svelte";

	let { class: className = "", ...rest }: ComponentProps<typeof Collapsible> = $props();
</script>

<Collapsible class="flat-disclosure {className}" {...rest} />

<style>
	/* The kit's rules live in `@layer ca.components`; unlayered rules beat them whatever the specificity. */
	:global(.flat-disclosure .ca-collapsible__trigger) {
		width: auto;
		align-self: flex-start;
		gap: 6px;
		padding: 4px 8px 4px 0;
		background: none;
		border: none;
		border-radius: 6px;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	:global(.flat-disclosure .ca-collapsible__trigger:hover) {
		background: none;
		color: var(--ca-text-primary);
	}
	:global(.flat-disclosure .ca-collapsible__title) {
		font-weight: 550;
	}
	:global(.flat-disclosure .ca-collapsible__body) {
		padding: 4px 0 2px;
		border: none;
		border-radius: 0;
	}
</style>
