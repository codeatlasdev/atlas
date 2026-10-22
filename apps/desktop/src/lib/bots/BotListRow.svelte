<script lang="ts">
	import type { Bot } from "#lib/stores/bots.svelte.ts";
	import BotFace from "#lib/bots/BotFace.svelte";
	import type { BotState as FaceState } from "#lib/bots/botFace.ts";
	import { roleTag, previewIcon } from "#lib/bots/botsLogic.ts";
	import { rowIn } from "#lib/bots/motion.ts";
	import { crossfade } from "svelte/transition";
	import { cubicOut } from "svelte/easing";
	import { agentLogo } from "#lib/agentLogos.ts";
	import { Check, CircleX, Ellipsis, TriangleAlert } from "@lucide/svelte";

	let {
		bot,
		active = false,
		unread = false,
		onclick,
		onContextAction,
	}: {
		bot: Bot;
		active?: boolean;
		unread?: boolean;
		onclick?: () => void;
		onContextAction?: (action: "profile" | "routine" | "delete") => void;
	} = $props();

	const faceState = $derived<FaceState>(
		bot.state === "needs_you" ? "needs_you" : bot.state === "working" ? "working" : "idle",
	);

	const tag = $derived(roleTag(bot.role));
	const icon = $derived(previewIcon(bot.last, bot.state));
	const isWorking = $derived(bot.state === "working");
	const logo = $derived(agentLogo(bot.agent));

	const [send, receive] = crossfade({
		duration: 200,
		easing: cubicOut,
		fallback(node) {
			return { duration: 150, css: (t) => `opacity: ${t}` };
		},
	});

	let menuOpen = $state(false);

	function fmtTime(ts?: number): string {
		if (!ts) return "";
		const d = new Date(ts * 1000);
		const now = new Date();
		if (d.toDateString() === now.toDateString()) {
			return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
		}
		return d.toLocaleDateString([], { month: "short", day: "numeric" });
	}

	function handleContextAction(action: "profile" | "routine" | "delete") {
		menuOpen = false;
		onContextAction?.(action);
	}
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="row-wrap" class:active class:unread onmouseleave={() => (menuOpen = false)} in:rowIn>
	<button class="row" class:active class:unread {onclick} aria-current={active ? "page" : undefined}>
		<!-- Avatar with presence badge -->
		<div class="avatar-wrap">
			{#if isWorking}
				<div class="avatar-dots" in:receive={{ key: bot.id }} out:send={{ key: bot.id }}>
					<span class="dot"></span>
					<span class="dot"></span>
					<span class="dot"></span>
				</div>
			{:else}
				<div in:receive={{ key: bot.id }} out:send={{ key: bot.id }}>
					<BotFace seed={bot.id} look={bot.look} state={faceState} size={32} />
					{#if logo}
						<span class="engine-badge" aria-label="{bot.agent} engine">
							{@html logo.dark ?? logo.light}
						</span>
					{/if}
				</div>
			{/if}
			<span
				class="presence"
				class:green={faceState === "working"}
				class:amber={faceState === "needs_you"}
				aria-hidden="true"
			></span>
		</div>

		<div class="row-body">
			<div class="row-top">
				<span class="name" class:bold={unread}>{bot.name}</span>
				{#if tag}
					<span class="role-tag">{tag}</span>
				{/if}
				<span class="time">{fmtTime(bot.last?.ts)}</span>
			</div>
			<div class="row-bottom">
				{#if isWorking}
					<span class="preview shimmer">working…</span>
				{:else}
					{#if icon}
						<span class="preview-icon" aria-hidden="true">
							{#if icon === "needs_you"}
								<TriangleAlert size={11} />
							{:else if icon === "done"}
								<Check size={11} />
							{:else if icon === "failed"}
								<CircleX size={11} />
							{/if}
						</span>
					{/if}
					<span class="preview" class:bold={unread}>{bot.last?.text ?? "No tasks yet"}</span>
				{/if}
				{#if unread}
					<span class="unread-badge" aria-label="Unread messages"></span>
				{:else if bot.state === "needs_you"}
					<span class="needs-badge" aria-label="Needs your attention">!</span>
				{/if}
			</div>
		</div></button
	>

	<!-- context menu, shown on row hover -->
	{#if onContextAction}
		<div class="menu-wrap">
			<button
				class="menu-btn"
				aria-label="More options for {bot.name}"
				aria-expanded={menuOpen}
				onclick={(e) => {
					e.stopPropagation();
					menuOpen = !menuOpen;
				}}><Ellipsis size={14} /></button
			>
			{#if menuOpen}
				<div class="menu-dropdown" role="menu">
					<button role="menuitem" class="menu-item" onclick={() => handleContextAction("profile")}
						>Profile</button
					>
					<button role="menuitem" class="menu-item" onclick={() => handleContextAction("routine")}
						>Run routine</button
					>
					<button role="menuitem" class="menu-item danger" onclick={() => handleContextAction("delete")}
						>Delete</button
					>
				</div>
			{/if}
		</div>
	{/if}
</div>

<style>
	.row-wrap {
		position: relative;
		display: flex;
		align-items: center;
		border-radius: 8px;
		margin: 0 4px 1px;
		transition: background 80ms;
	}
	.row-wrap:hover {
		background: var(--ca-vibrancy-fill);
	}
	.row-wrap.active {
		background: color-mix(in oklab, var(--ca-brand-main) 15%, transparent);
	}

	.row {
		flex: 1;
		min-width: 0;
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 6px 8px;
		text-align: left;
		background: none;
		border: none;
		border-radius: 8px;
		cursor: default;
	}

	/* Avatar wrapper with presence badge */
	.avatar-wrap {
		position: relative;
		flex: none;
		width: 32px;
		height: 32px;
	}
	.engine-badge {
		position: absolute;
		top: -2px;
		right: -2px;
		width: 12px;
		height: 12px;
		border-radius: 3px;
		overflow: hidden;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--ca-surface-sunken);
		border: 1.5px solid var(--ca-surface-sunken);
		pointer-events: none;
	}
	.engine-badge :global(svg) {
		width: 10px;
		height: 10px;
	}

	/* Presence dot: bottom-right of avatar, ring cut in sidebar color */
	.presence {
		position: absolute;
		bottom: -1px;
		right: -1px;
		width: 10px;
		height: 10px;
		border-radius: 50%;
		background: var(--ca-text-disabled, #888);
		border: 2px solid var(--ca-surface-sunken);
		pointer-events: none;
	}
	.presence.green {
		background: var(--ca-semantic-success, #2f9e44);
	}
	.presence.amber {
		background: var(--ca-semantic-warning, #e67700);
	}

	/* Dots shown while working (crossfade with face) */
	.avatar-dots {
		width: 32px;
		height: 32px;
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 4px;
		border-radius: 8px;
		background: color-mix(in oklab, var(--ca-outline-subtle) 40%, transparent);
	}
	.dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--ca-text-secondary);
		animation: dot-bounce 1.2s ease-in-out infinite;
	}
	.dot:nth-child(2) {
		animation-delay: 0.15s;
	}
	.dot:nth-child(3) {
		animation-delay: 0.3s;
	}
	@keyframes dot-bounce {
		0%,
		80%,
		100% {
			transform: scale(0.8);
			opacity: 0.5;
		}
		40% {
			transform: scale(1.1);
			opacity: 1;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.dot {
			animation: none;
			opacity: 0.8;
		}
	}

	.row-body {
		flex: 1;
		min-width: 0;
	}

	.row-top {
		display: flex;
		align-items: baseline;
		gap: 4px;
		margin-bottom: 1px;
	}

	.name {
		font-size: 13px;
		font-weight: 500;
		color: var(--ca-text-primary);
		flex: none;
		max-width: 90px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.name.bold {
		font-weight: 700;
	}

	.role-tag {
		font-size: 10px;
		font-weight: 500;
		color: var(--ca-text-secondary);
		background: color-mix(in oklab, var(--ca-outline-subtle) 60%, transparent);
		border-radius: 10px;
		padding: 1px 5px;
		white-space: nowrap;
		flex: none;
		max-width: 70px;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.time {
		font-size: 10.5px;
		color: var(--ca-text-secondary);
		margin-left: auto;
		flex: none;
	}

	.row-bottom {
		display: flex;
		align-items: center;
		gap: 3px;
	}

	.preview-icon {
		font-size: 10px;
		color: var(--ca-text-secondary);
		flex: none;
	}

	.preview {
		font-size: 12px;
		color: var(--ca-text-secondary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		flex: 1;
	}
	.preview.bold {
		color: var(--ca-text-primary);
		font-weight: 500;
	}

	/* Shimmer "working…" animation */
	.shimmer {
		background: linear-gradient(
			90deg,
			var(--ca-text-secondary) 0%,
			var(--ca-text-primary) 40%,
			var(--ca-text-primary) 60%,
			var(--ca-text-secondary) 100%
		);
		background-size: 200% 100%;
		background-clip: text;
		-webkit-background-clip: text;
		color: transparent;
		animation: shimmer 2s ease-in-out infinite;
	}
	@keyframes shimmer {
		0% {
			background-position: 200% 0;
		}
		100% {
			background-position: -200% 0;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.shimmer {
			animation: none;
			color: var(--ca-text-secondary);
			background: none;
			-webkit-background-clip: initial;
			background-clip: initial;
		}
	}

	.unread-badge {
		flex: none;
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--ca-brand-main, #228be6);
	}

	.needs-badge {
		flex: none;
		min-width: 16px;
		height: 16px;
		border-radius: 8px;
		background: var(--ca-semantic-error, #e03131);
		color: var(--ca-surface-base);
		font-size: 9px;
		font-weight: 700;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 0 3px;
	}

	/* context menu */
	.menu-wrap {
		position: relative;
		display: none;
		flex: none;
		margin-right: 4px;
	}
	.row-wrap:hover .menu-wrap {
		display: flex;
	}

	.menu-btn {
		width: 24px;
		height: 24px;
		border-radius: 6px;
		border: none;
		background: none;
		color: var(--ca-text-secondary);
		cursor: default;
		font-size: 14px;
		display: flex;
		align-items: center;
		justify-content: center;
	}
	.menu-btn:hover {
		background: var(--ca-outline-subtle);
		color: var(--ca-text-primary);
	}

	.menu-dropdown {
		position: absolute;
		right: 0;
		top: 100%;
		z-index: 50;
		background: var(--ca-surface-overlay, #2a2a2a);
		border: 0.5px solid var(--ca-outline-subtle);
		border-radius: 8px;
		padding: 4px;
		min-width: 140px;
		box-shadow: 0 4px 16px rgb(0 0 0 / 0.3);
	}

	.menu-item {
		display: block;
		width: 100%;
		padding: 6px 10px;
		text-align: left;
		border: none;
		background: none;
		border-radius: 5px;
		font-size: 12.5px;
		color: var(--ca-text-primary);
		cursor: default;
	}
	.menu-item:hover {
		background: var(--ca-vibrancy-fill);
	}
	.menu-item.danger {
		color: var(--ca-semantic-error, #e03131);
	}
</style>
