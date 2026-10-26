<script lang="ts">
	import { Tooltip } from "@codeatlasdev/ui";
	import { Play, RotateCw, ScrollText, Square, SquareTerminal } from "@lucide/svelte";
	import type { Component } from "svelte";
	import { guarded } from "#lib/components/tooltipGuard.ts";

	type Cmd = "start_service" | "stop_service" | "restart_service";
	type Btn = {
		label: string;
		tip: string;
		icon: Component<{ size?: number }>;
		size: number;
		run: () => void;
		off?: boolean;
	};

	/** Hover/focus button strip of a service card: lifecycle (when the stack is up), logs, terminal. */
	let {
		running,
		up,
		busy,
		dir,
		onaction,
		onlogs,
		onterminal,
	}: {
		running: boolean;
		/** The service is running or starting. */
		up: boolean;
		busy: boolean;
		dir: string | null | undefined;
		onaction: (cmd: Cmd) => void;
		onlogs: () => void;
		onterminal: () => void;
	} = $props();

	let buttons = $derived.by((): Btn[] => {
		const life: Btn[] = !running
			? []
			: up
				? [
						{
							label: "Restart",
							tip: "Restart (R)",
							icon: RotateCw,
							size: 13,
							run: () => onaction("restart_service"),
							off: busy,
						},
						{
							label: "Stop",
							tip: "Stop",
							icon: Square,
							size: 12,
							run: () => onaction("stop_service"),
							off: busy,
						},
					]
				: [
						{
							label: "Start",
							tip: "Start",
							icon: Play,
							size: 13,
							run: () => onaction("start_service"),
							off: busy,
						},
					];
		return [
			...life,
			{ label: "Logs", tip: "Logs (Enter)", icon: ScrollText, size: 13, run: onlogs },
			{
				label: "Terminal",
				tip: `Terminal in ${dir ?? "project root"}`,
				icon: SquareTerminal,
				size: 13,
				run: onterminal,
			},
		];
	});
</script>

<div class="btns">
	{#each buttons as b (b.label)}
		<Tooltip content={b.tip}>
			{#snippet triggerChild(props)}
				<!-- The kit's own wrapper button would nest this one; the control is the trigger. -->
				<button
					{...guarded(props)}
					class="b"
					disabled={b.off}
					aria-label={b.label}
					onclick={(e) => {
						e.stopPropagation();
						b.run();
					}}><b.icon size={b.size} /></button
				>
			{/snippet}
		</Tooltip>
	{/each}
</div>

<style>
	.btns {
		position: absolute;
		top: 8px;
		right: 8px;
		display: flex;
		gap: 2px;
		padding: 2px;
		border-radius: 8px;
		background: var(--ca-surface-raised);
		box-shadow: 0 0 0 0.5px var(--ca-outline-subtle);
		opacity: 0;
		transform: translateY(-2px);
		pointer-events: none;
		transition:
			opacity 160ms ease-out,
			transform 200ms cubic-bezier(0.2, 0.8, 0.2, 1);
	}
	:global(.card:hover) .btns,
	:global(.card.focused) .btns,
	:global(.card:focus-within) .btns {
		opacity: 1;
		transform: none;
		pointer-events: auto;
	}
	.b {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 26px;
		height: 26px;
		border: 0;
		border-radius: 6px;
		background: transparent;
		color: var(--ca-text-secondary);
	}
	.b:hover:not(:disabled) {
		background: var(--ca-surface-sunken);
		color: var(--ca-text-primary);
	}
	.b:disabled {
		color: var(--ca-text-disabled);
	}
	@media (prefers-reduced-motion: reduce) {
		.btns {
			transition: none;
		}
	}
</style>
