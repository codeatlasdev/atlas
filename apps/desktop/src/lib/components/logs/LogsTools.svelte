<script lang="ts">
	import { Switch } from "@codeatlasdev/ui";
	import {
		ChevronDown,
		ChevronUp,
		CircleAlert,
		Clock,
		Copy,
		Sparkles,
		TriangleAlert,
		WrapText,
	} from "@lucide/svelte";
	import type { Level } from "#lib/logs/model.ts";
	import { kbd } from "#lib/keys.ts";
	import { m } from "#lib/i18n/m.ts";

	let {
		showTime = $bindable(),
		wrap = $bindable(),
		follow = $bindable(),
		aiLines,
		onjump,
		onfollow,
		oncopy,
		oncopyai,
	}: {
		showTime: boolean;
		wrap: boolean;
		follow: boolean;
		/** Lines "Copy for AI" hands over (shown in its tooltip). */
		aiLines: number;
		/** Jump to the previous (-1) / next (1) line of at least `min` level. */
		onjump: (dir: 1 | -1, min: Level) => void;
		/** The Follow switch went on: scroll to the newest line. */
		onfollow: () => void;
		oncopy: () => void;
		oncopyai: () => void;
	} = $props();

	const jumps = $derived([
		{
			dir: -1,
			min: "error",
			cls: "err",
			Icon: CircleAlert,
			title: m.logs_tools_previous_error_tip({ key: kbd("shift-e") }),
			label: m.logs_tools_previous_error(),
		},
		{
			dir: 1,
			min: "error",
			cls: "err",
			Icon: CircleAlert,
			title: m.logs_tools_next_error_tip({ key: kbd("e") }),
			label: m.logs_tools_next_error(),
		},
		{
			dir: -1,
			min: "warn",
			cls: "warn",
			Icon: TriangleAlert,
			title: m.logs_tools_previous_warning_tip({ key: kbd("shift-w") }),
			label: m.logs_tools_previous_warning(),
		},
		{
			dir: 1,
			min: "warn",
			cls: "warn",
			Icon: TriangleAlert,
			title: m.logs_tools_next_warning_tip({ key: kbd("w") }),
			label: m.logs_tools_next_warning(),
		},
	] as const);
</script>

<div class="jump">
	{#each jumps as j (j.dir + j.min)}
		<button class="icon {j.cls}" title={j.title} aria-label={j.label} onclick={() => onjump(j.dir, j.min)}>
			<j.Icon size={13} />
			{#if j.dir === -1}<ChevronUp size={11} />{:else}<ChevronDown size={11} />{/if}
		</button>
	{/each}
</div>
<div class="right">
	<button
		class="icon"
		class:on={showTime}
		title={m.logs_tools_timestamps_tip({ key: kbd("t") })}
		aria-label={m.logs_tools_timestamps()}
		aria-pressed={showTime}
		onclick={() => (showTime = !showTime)}
	>
		<Clock size={14} />
	</button>
	<button
		class="icon"
		class:on={wrap}
		title={m.logs_tools_wrap_long_lines()}
		aria-label={m.logs_tools_wrap_long_lines()}
		aria-pressed={wrap}
		onclick={() => (wrap = !wrap)}
	>
		<WrapText size={14} />
	</button>
	<Switch label={m.logs_tools_follow()} bind:checked={follow} onchange={() => follow && onfollow()} />
	<button
		class="icon"
		onclick={oncopy}
		title={m.logs_tools_copy_filtered_lines()}
		aria-label={m.logs_tools_copy_filtered_lines()}
	>
		<Copy size={14} />
	</button>
	<button
		class="icon"
		onclick={oncopyai}
		title={m.logs_tools_copy_last_lines_for_ai({ lines: aiLines })}
		aria-label={m.logs_tools_copy_for_ai()}
	>
		<Sparkles size={14} />
	</button>
</div>

<style>
	.jump {
		display: flex;
		gap: 2px;
		flex-wrap: wrap;
		align-items: center;
	}
	.right {
		margin-left: auto;
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.icon {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 1px;
		min-width: 26px;
		height: 26px;
		border: 0;
		border-radius: 7px;
		background: transparent;
		color: var(--ca-text-secondary);
		cursor: pointer;
	}
	.icon.on {
		background: var(--ca-brand-fill);
		color: var(--ca-brand-contrast);
	}
	.icon:hover:not(.on) {
		background: var(--ca-action-hover);
	}
	.icon.err {
		color: var(--ca-semantic-error);
	}
	.icon.warn {
		color: var(--ca-semantic-warning);
	}
</style>
