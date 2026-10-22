<!--
  BotArtChat — step 3 illustration.
  A real chat thread animates: user message → TypingIndicator → steps tick in → markdown reply.
  8 s loop; pauses on document.hidden.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import { reducedMotion } from "#lib/bots/motion.ts";
	import { Check } from "@lucide/svelte";

	const PERIOD = 8000;
	let t = $state(reducedMotion ? 0.75 : 0.75);
	let paused = $state(false);

	function seg(x: number, a: number, b: number): number {
		return Math.max(0, Math.min(1, (x - a) / (b - a)));
	}

	// What's visible at each clock position
	const showUserMsg = $derived(t >= 0.08);
	const showTyping = $derived(t >= 0.22 && t < 0.72);
	const showStep1 = $derived(t >= 0.32);
	const showStep2 = $derived(t >= 0.48);
	const showStep3 = $derived(t >= 0.6);
	const showReply = $derived(t >= 0.72);

	// Dot pulse for typing (3-dot cycle)
	const dotPhase = $derived(Math.floor(t * 48) % 3);

	onMount(() => {
		if (reducedMotion) return;
		const startTime = performance.now() - PERIOD * 0.75;
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

<div class="art-chat" aria-hidden="true">
	<div class="thread">
		<!-- User message -->
		{#if showUserMsg}
			<div class="msg-row user-row">
				<div class="bubble user-bubble">
					Create a migration for<br />users.email_idx
				</div>
			</div>
		{/if}

		<!-- Bot row: avatar + content -->
		{#if showTyping || showStep1}
			<div class="msg-row bot-row">
				<div class="avatar-col">
					<div class="avatar-dot"></div>
				</div>
				<div class="bot-content">
					<div class="author-line">
						<span class="author-name">Reviewer</span>
						<span class="author-time">11:24 PM</span>
					</div>

					<!-- Typing indicator -->
					{#if showTyping}
						<div class="typing-dots">
							<span class="dot" class:active={dotPhase === 0}></span>
							<span class="dot" class:active={dotPhase === 1}></span>
							<span class="dot" class:active={dotPhase === 2}></span>
						</div>
					{/if}

					<!-- Steps tick in -->
					{#if showStep1}
						<div class="step-row">
							<span class="step-icon done"><Check size={10} /></span>
							<span class="step-text">Reading schema…</span>
						</div>
					{/if}
					{#if showStep2}
						<div class="step-row">
							<span class="step-icon done"><Check size={10} /></span>
							<span class="step-text">Writing migration…</span>
						</div>
					{/if}
					{#if showStep3}
						<div class="step-row">
							<span class="step-icon done"><Check size={10} /></span>
							<span class="step-text">Done in 340 ms</span>
						</div>
					{/if}

					<!-- Markdown reply -->
					{#if showReply}
						<div class="reply">
							Migration created:<br />
							<code>20241022_add_users_email_idx.sql</code><br /><br />
							Ready to run <code>bun run db:migrate</code>
						</div>
					{/if}
				</div>
			</div>
		{/if}
	</div>
</div>

<style>
	.art-chat {
		width: 100%;
		height: 100%;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 24px 16px;
		box-sizing: border-box;
	}

	.thread {
		width: min(480px, 92%);
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

	/* User message: right-aligned bubble */
	.user-row {
		justify-content: flex-end;
	}

	.bubble {
		padding: 8px 12px;
		border-radius: 14px 14px 2px 14px;
		max-width: 72%;
		word-break: break-word;
	}

	.user-bubble {
		background: var(--ca-accent, #4f8ef7);
		color: var(--ca-surface-base);
		border-radius: 14px 14px 2px 14px;
	}

	/* Bot message */
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

	/* Typing dots */
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
		background: var(--ca-text-primary);
	}

	/* Step rows */
	.step-row {
		display: flex;
		align-items: center;
		gap: 5px;
		animation: msg-in 180ms both;
	}

	.step-icon {
		display: flex;
		align-items: center;
		color: var(--ca-text-secondary);
	}

	.step-icon.done {
		color: var(--ca-semantic-success);
	}

	.step-text {
		font-size: 11.5px;
		color: var(--ca-text-secondary);
	}

	/* Markdown reply */
	.reply {
		margin-top: 2px;
		color: var(--ca-text-primary);
		font-size: 12px;
		animation: msg-in 240ms both;
	}

	.reply code {
		font-family: monospace;
		font-size: 11px;
		background: var(--ca-surface-sunken);
		padding: 1px 5px;
		border-radius: 3px;
		color: var(--ca-text-primary);
	}

	@media (prefers-reduced-motion: reduce) {
		.msg-row,
		.typing-dots,
		.step-row,
		.reply {
			animation: none;
		}
	}
</style>
