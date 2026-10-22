<!--
  BotArtApproval — step 4 illustration.
  Timeline: user task → approval card arrives → Allow pressed → "Allowed by you" → result.
  A phone-style notification card slides in from the top-right corner.
  8 s loop; pauses on document.hidden.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import { reducedMotion } from "#lib/bots/motion.ts";
	import { Check, Bell } from "@lucide/svelte";

	const PERIOD = 8000;
	let t = $state(reducedMotion ? 0.55 : 0.76);
	let paused = $state(false);

	function seg(x: number, a: number, b: number): number {
		return Math.max(0, Math.min(1, (x - a) / (b - a)));
	}
	function ease(p: number): number {
		return p < 0.5 ? 2 * p * p : 1 - (-2 * p + 2) ** 2 / 2;
	}
	function segs(x: number, a: number, b: number): number {
		return ease(seg(x, a, b));
	}

	// Stages
	// 0-0.10: empty, then user msg appears
	// 0.10-0.28: approval card slides in (amber border)
	// 0.28-0.42: Allow button press animation
	// 0.42-0.56: "Allowed by you" replaces card
	// 0.48-0.60: typing indicator
	// 0.60-0.76: result appears
	// 0.76-1.0: hold

	const showUserMsg = $derived(t >= 0.08);
	const showApproval = $derived(t >= 0.12 && t < 0.42);
	const allowPressed = $derived(t >= 0.3 && t < 0.42);
	const allowScale = $derived(allowPressed ? 1 - segs(t, 0.3, 0.38) * 0.08 : 1);
	const showAllowed = $derived(t >= 0.42);
	const showTyping2 = $derived(t >= 0.48 && t < 0.6);
	const showResult = $derived(t >= 0.6);

	// Notification card: slides in at t=0.38, stays
	const notifVisible = $derived(t >= 0.38);
	const notifX = $derived(notifVisible ? 1 - segs(t, 0.38, 0.5) : 0);

	// Approval card entrance
	const approvalX = $derived(t >= 0.12 ? 1 - segs(t, 0.12, 0.22) : 0);
	const approvalOpacity = $derived(t >= 0.12 && t < 0.42 ? segs(t, 0.12, 0.22) : 0);

	// Typing dot pulse
	const dotPhase = $derived(Math.floor(t * 36) % 3);

	onMount(() => {
		if (reducedMotion) return;
		const startTime = performance.now() - PERIOD * 0.76;
		const id = setInterval(() => {
			if (paused) return;
			t = ((performance.now() - startTime) % PERIOD) / PERIOD;
		}, 50);
		const onVis = () => {
			paused = document.hidden;
		};
		document.addEventListener("visibilitychange", onVis);
		return () => {
			clearInterval(id);
			document.removeEventListener("visibilitychange", onVis);
		};
	});
</script>

<div class="art-approval" aria-hidden="true">
	<div class="thread">
		<!-- User message -->
		{#if showUserMsg}
			<div class="msg-row user-row">
				<div class="bubble user-bubble">Run the pending migrations</div>
			</div>
		{/if}

		<!-- Bot row with approval or result -->
		{#if showApproval || showAllowed || showTyping2 || showResult}
			<div class="msg-row bot-row">
				<div class="avatar-col">
					<div class="avatar-dot"></div>
				</div>
				<div class="bot-content">
					<div class="author-line">
						<span class="author-name">Reviewer</span>
						<span class="author-time">11:24 PM</span>
					</div>

					<!-- Approval card -->
					{#if showApproval}
						<div
							class="approval-card"
							style:transform="translateX({approvalX * 20}px)"
							style:opacity={approvalOpacity}
						>
							<div class="approval-label">Wants to run</div>
							<code class="approval-cmd">bun run db:migrate</code>
							<div class="approval-btns">
								<button class="btn-deny" tabindex="-1">Deny</button>
								<button class="btn-allow" tabindex="-1" style:transform="scale({allowScale})">Allow</button>
							</div>
						</div>
					{/if}

					<!-- Allowed confirmation -->
					{#if showAllowed}
						<div class="allowed-row">
							<span class="allowed-icon"><Check size={11} /></span>
							<span class="allowed-text">Allowed by you · now</span>
						</div>
					{/if}

					<!-- Typing dots -->
					{#if showTyping2}
						<div class="typing-dots">
							<span class="dot" class:active={dotPhase === 0}></span>
							<span class="dot" class:active={dotPhase === 1}></span>
							<span class="dot" class:active={dotPhase === 2}></span>
						</div>
					{/if}

					<!-- Result -->
					{#if showResult}
						<div class="reply">
							Migration applied.<br />
							<span class="perf">840 ms → 9 ms</span>
						</div>
					{/if}
				</div>
			</div>
		{/if}
	</div>

	<!-- Phone notification card (top-right corner) -->
	{#if notifVisible}
		<div class="notif-card" style:transform="translateX({notifX * 120}px)" style:opacity={1 - notifX}>
			<div class="notif-icon-wrap">
				<Bell size={13} />
			</div>
			<div class="notif-body">
				<div class="notif-app">Atlas Bot</div>
				<div class="notif-msg">Reviewer approved the migration</div>
			</div>
		</div>
	{/if}
</div>

<style>
	.art-approval {
		width: 100%;
		height: 100%;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 16px;
		box-sizing: border-box;
		position: relative;
	}

	.thread {
		width: min(380px, 100%);
		display: flex;
		flex-direction: column;
		gap: 8px;
		font-size: 12.5px;
		line-height: 1.5;
	}

	.msg-row {
		display: flex;
		align-items: flex-start;
		gap: 8px;
		animation: msg-in 220ms both;
	}

	@keyframes msg-in {
		from {
			opacity: 0;
			transform: translateY(6px);
		}
		to {
			opacity: 1;
			transform: none;
		}
	}

	.user-row {
		justify-content: flex-end;
	}

	.bubble {
		padding: 8px 12px;
		border-radius: 14px 14px 2px 14px;
		max-width: 72%;
	}

	.user-bubble {
		background: var(--ca-accent, #4f8ef7);
		color: var(--ca-surface-base);
	}

	.bot-row {
		justify-content: flex-start;
	}

	.avatar-col {
		flex-shrink: 0;
		width: 28px;
		padding-top: 2px;
	}

	.avatar-dot {
		width: 28px;
		height: 28px;
		border-radius: 50%;
		background: var(--ca-surface-sunken);
		border: 1.5px solid var(--ca-border-subtle);
	}

	.bot-content {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.author-line {
		display: flex;
		align-items: baseline;
		gap: 6px;
	}

	.author-name {
		font-weight: 600;
		font-size: 12.5px;
		color: var(--ca-text-primary);
	}

	.author-time {
		font-size: 10.5px;
		color: var(--ca-text-secondary);
	}

	/* Approval card */
	.approval-card {
		background: var(--ca-surface-base);
		border: 1.5px solid var(--ca-warning, #f59e0b);
		border-radius: 8px;
		padding: 10px 12px;
		display: flex;
		flex-direction: column;
		gap: 6px;
		will-change: transform, opacity;
	}

	.approval-label {
		font-size: 11px;
		color: var(--ca-text-secondary);
	}

	.approval-cmd {
		font-family: monospace;
		font-size: 12px;
		color: var(--ca-text-primary);
		background: var(--ca-surface-sunken);
		padding: 3px 7px;
		border-radius: 4px;
		display: block;
	}

	.approval-btns {
		display: flex;
		gap: 6px;
		justify-content: flex-end;
	}

	.btn-deny,
	.btn-allow {
		padding: 4px 12px;
		border-radius: 5px;
		font-size: 11.5px;
		font-weight: 500;
		border: none;
		cursor: default;
		will-change: transform;
	}

	.btn-deny {
		background: var(--ca-surface-sunken);
		color: var(--ca-text-secondary);
		border: 1px solid var(--ca-border-subtle);
	}

	.btn-allow {
		background: var(--ca-accent, #4f8ef7);
		color: var(--ca-surface-base);
		transform-origin: center;
	}

	/* Allowed row */
	.allowed-row {
		display: flex;
		align-items: center;
		gap: 5px;
		animation: msg-in 180ms both;
	}

	.allowed-icon {
		color: var(--ca-semantic-success);
		display: flex;
		align-items: center;
	}

	.allowed-text {
		font-size: 11.5px;
		color: var(--ca-text-secondary);
	}

	/* Typing */
	.typing-dots {
		display: flex;
		gap: 4px;
		padding: 4px 0;
		animation: msg-in 150ms both;
	}

	.typing-dots .dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--ca-text-secondary);
		opacity: 0.35;
		transition: opacity 100ms;
	}

	.typing-dots .dot.active {
		opacity: 1;
	}

	/* Result */
	.reply {
		color: var(--ca-text-primary);
		font-size: 12px;
		animation: msg-in 240ms both;
	}

	.perf {
		font-family: monospace;
		font-size: 11px;
		color: var(--ca-semantic-success);
	}

	/* Phone notification card */
	.notif-card {
		position: absolute;
		top: 20px;
		right: 20px;
		display: flex;
		align-items: center;
		gap: 8px;
		background: var(--ca-surface-overlay, var(--ca-surface-base));
		border: 1px solid var(--ca-border-subtle);
		border-radius: 12px;
		padding: 8px 12px;
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.12);
		max-width: 200px;
		will-change: transform, opacity;
	}

	.notif-icon-wrap {
		flex-shrink: 0;
		width: 26px;
		height: 26px;
		border-radius: 6px;
		background: var(--ca-accent, #4f8ef7);
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--ca-surface-base);
	}

	.notif-body {
		min-width: 0;
	}

	.notif-app {
		font-size: 10px;
		font-weight: 600;
		color: var(--ca-text-secondary);
	}

	.notif-msg {
		font-size: 11px;
		color: var(--ca-text-primary);
		line-height: 1.3;
	}

	@media (prefers-reduced-motion: reduce) {
		.msg-row,
		.allowed-row,
		.typing-dots,
		.reply {
			animation: none;
		}
	}
</style>
