<script lang="ts">
	/**
	 * ActionTextAnswer — the free-text answer block of an action card: one input, Submit and Cancel. Written once
	 * for the secret (password), the labelled input and the generic "response needed" kinds.
	 */
	import { Button } from "@codeatlasdev/ui";

	interface Props {
		value: string;
		type?: "text" | "password";
		placeholder?: string;
		label: string;
		/** Enter in the field submits (the secret field does not: a stray Enter must not send a half-typed secret). */
		submitOnEnter?: boolean;
		submitting: boolean;
		onchange: (value: string) => void;
		onsubmit: (value: string) => void;
		oncancel: () => void;
	}

	let {
		value,
		type = "text",
		placeholder,
		label,
		submitOnEnter = false,
		submitting,
		onchange,
		onsubmit,
		oncancel,
	}: Props = $props();

	function submit() {
		const v = value.trim();
		if (v) onsubmit(v);
	}
</script>

<input
	{type}
	class="action-input"
	{placeholder}
	{value}
	oninput={(e) => onchange((e.currentTarget as HTMLInputElement).value)}
	onkeydown={(e) => {
		if (submitOnEnter && e.key === "Enter") submit();
	}}
	aria-label={label}
/>
<div class="action-btns">
	<Button size="sm" disabled={!value.trim() || submitting} onclick={submit}>Submit</Button>
	<Button variant="ghost" size="sm" onclick={oncancel}>Cancel</Button>
</div>

<style>
	.action-input {
		background: var(--ca-surface-base);
		border: 0.5px solid var(--ca-outline-subtle);
		border-radius: 8px;
		padding: 6px 10px;
		font-size: 13px;
		color: var(--ca-text-primary);
		outline: none;
		width: 100%;
		box-sizing: border-box;
	}
	.action-input:focus {
		border-color: var(--ca-outline-primary);
	}
	.action-btns {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
	}
	.action-btns :global(button) {
		white-space: nowrap;
	}
</style>
