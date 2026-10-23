<!--
  BotArtApproval — step 4 onboarding illustration.
  "It asks before anything risky": crewmate goes needs_you, approval
  card + phone slide in, tap ring lands on Allow on the phone, card
  becomes "Allowed from your phone", face recovers, done bubble lands.
  10 s loop. Reduced-motion: needs_you face + card + phone, no tap.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import BotFace from "#lib/bots/BotFace.svelte";
	import type { BotState } from "#lib/bots/botFace.ts";
	import { reducedMotion } from "#lib/bots/motion.ts";
	import { Smartphone } from "@lucide/svelte";

	let stageW = $state(640);
	const k = $derived(Math.max(0.6, stageW / 640));
	const faceSize = $derived(Math.round(120 * k));

	const PERIOD = 10000;
	let t = $state(reducedMotion ? 0.32 : 0);
	let ph = $state(0);
	let paused = $state(false);

	function seg(x: number, a: number, b: number): number {
		const p = Math.max(0, Math.min(1, (x - a) / (b - a)));
		return p < 0.5 ? 2 * p * p : 1 - (-2 * p + 2) ** 2 / 2;
	}

	// Timeline:
	// 0.00–0.10  face working
	// 0.10–0.52  face needs_you, card visible, phone slides in at 0.28
	// 0.42–0.52  tap ring on Allow button on phone
	// 0.52–0.68  face working, card → "Allowed from your phone"
	// 0.66–0.88  done bubble
	// 0.88–1.00  hold
	const botState = $derived<BotState>(reducedMotion || (t >= 0.1 && t < 0.52) ? "needs_you" : "working");

	const showCard = $derived(reducedMotion || t >= 0.12);
	const cardAllowed = $derived(reducedMotion ? false : t >= 0.52 && t < 0.96);

	const showPhone = $derived(reducedMotion || t >= 0.28);

	// Tap ring: enters DOM for one beat, CSS animates it once
	const showTap = $derived(!reducedMotion && t >= 0.42 && t < 0.52);

	const showDone = $derived(!reducedMotion && t >= 0.66 && t < 0.9);

	// Gentle float
	const floatY = $derived(reducedMotion ? 0 : Math.sin(ph) * 7);

	// Phone slide-in offset: 0 when visible (CSS handles the entrance)
	// We just gate with {#if showPhone} so the pop-in animation triggers on entry.

	onMount(() => {
		if (reducedMotion) return;
		const t0 = performance.now();
		let last = t0;
		let raf: number;
		const tick = (now: number) => {
			if (!paused) {
				t = ((now - t0) % PERIOD) / PERIOD;
				ph += ((now - last) / 1000) * (Math.PI * 0.44);
			}
			last = now;
			raf = requestAnimationFrame(tick);
		};
		raf = requestAnimationFrame(tick);
		const onVis = () => {
			paused = document.hidden;
			if (!paused) last = performance.now();
		};
		document.addEventListener("visibilitychange", onVis);
		return () => {
			cancelAnimationFrame(raf);
			document.removeEventListener("visibilitychange", onVis);
		};
	});
</script>

<div class="scene" aria-hidden="true">
	<div class="stage" bind:clientWidth={stageW}>
		<div class="content-row" style:gap="{Math.round(20 * k)}px">
			<!-- Left: face + approval card -->
			<div class="left-col" style:transform="translateY({floatY}px)">
				<div class="face-pop">
					<BotFace seed="art-guard" state={botState} size={faceSize} />
				</div>

				{#if showCard}
					<div class="approval-card card-pop">
						<div class="card-command">
							<span class="cmd-label">Wants to run</span>
							<code class="cmd-code">git push origin main</code>
						</div>

						{#if cardAllowed}
							<div class="allowed-row allowed-pop">
								<div class="allowed-dot"></div>
								<span>Allowed from your phone</span>
							</div>
						{:else}
							<div class="action-row">
								<div class="pill action deny">Deny</div>
								<div class="pill action allow">Allow</div>
							</div>
						{/if}
					</div>
				{/if}
			</div>

			<!-- Right: phone notification -->
			{#if showPhone}
				<div class="phone-wrap phone-pop">
					<!-- Phone shell -->
					<div class="phone" style:width="{Math.round(120 * k)}px" style:height="{Math.round(200 * k)}px">
						<!-- Status bar -->
						<div class="phone-status">
							<span class="phone-time">9:41</span>
							<Smartphone size={Math.round(9 * k)} strokeWidth={1.5} />
						</div>

						<!-- Notification card on lock screen -->
						<div class="notif-card">
							<div class="notif-app">Atlas Bot</div>
							<div class="notif-title">Reviewer needs you</div>
							<div class="notif-actions">
								<div class="notif-btn">Deny</div>
								<div class="notif-btn allow" style:position="relative">
									Allow
									<!-- Tap ring: expands and fades when visible -->
									{#if showTap}
										<span class="tap-ring"></span>
									{/if}
								</div>
							</div>
						</div>
					</div>
				</div>
			{/if}
		</div>

		<!-- Done bubble: appears below after approval -->
		{#if showDone}
			<div class="done-bubble done-pop">Done · pushed to main</div>
		{/if}
	</div>
</div>

<style>
	.scene {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		overflow: hidden;
	}

	.stage {
		position: absolute;
		inset: 0;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 16px;
	}

	/* Main row: face+card left, phone right */
	.content-row {
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.left-col {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 12px;
		will-change: transform;
	}

	/* ── Animations ─────────────────────────────────────────────────────── */

	@keyframes pop-in {
		from {
			opacity: 0;
			transform: scale(0.82);
		}
		to {
			opacity: 1;
			transform: scale(1);
		}
	}

	@keyframes slide-in-right {
		from {
			opacity: 0;
			transform: translateX(18px) scale(0.94);
		}
		to {
			opacity: 1;
			transform: translateX(0) scale(1);
		}
	}

	@keyframes tap-expand {
		0% {
			transform: translate(-50%, -50%) scale(0.4);
			opacity: 0.8;
		}
		100% {
			transform: translate(-50%, -50%) scale(2.4);
			opacity: 0;
		}
	}

	.face-pop {
		animation: pop-in 420ms 0ms both cubic-bezier(0.34, 1.56, 0.64, 1);
	}

	.card-pop {
		animation: pop-in 360ms 60ms both cubic-bezier(0.34, 1.56, 0.64, 1);
	}

	.phone-pop {
		animation: slide-in-right 400ms 0ms both cubic-bezier(0.22, 1, 0.36, 1);
	}

	.allowed-pop {
		animation: pop-in 300ms 0ms both cubic-bezier(0.34, 1.56, 0.64, 1);
	}

	.done-pop {
		animation: pop-in 320ms 0ms both cubic-bezier(0.34, 1.56, 0.64, 1);
	}

	@media (prefers-reduced-motion: reduce) {
		.face-pop,
		.card-pop,
		.phone-pop,
		.allowed-pop,
		.done-pop {
			animation: none;
		}
	}

	/* ── Approval card ───────────────────────────────────────────────────── */

	.approval-card {
		background: var(--ca-surface-raised, rgba(255, 255, 255, 0.06));
		border: 1px solid var(--ca-border-subtle, rgba(255, 255, 255, 0.1));
		border-radius: 12px;
		padding: 12px 14px;
		display: flex;
		flex-direction: column;
		gap: 10px;
		max-width: 220px;
		pointer-events: none;
		user-select: none;
	}

	.card-command {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.cmd-label {
		font-size: 11px;
		color: var(--ca-text-secondary, rgba(255, 255, 255, 0.5));
	}

	.cmd-code {
		font-family: ui-monospace, monospace;
		font-size: 12px;
		color: var(--ca-text-primary);
		background: rgba(255, 255, 255, 0.05);
		border-radius: 6px;
		padding: 4px 8px;
		white-space: nowrap;
	}

	.action-row {
		display: flex;
		gap: 6px;
		justify-content: flex-end;
	}

	.pill {
		white-space: nowrap;
		padding: 4px 12px;
		background: var(--ca-surface-raised, rgba(255, 255, 255, 0.07));
		border: 1px solid var(--ca-border-subtle, rgba(255, 255, 255, 0.1));
		border-radius: 999px;
		font-size: 12px;
		font-weight: 500;
		color: var(--ca-text-primary);
		pointer-events: none;
		user-select: none;
	}

	.action.deny {
		color: var(--ca-text-secondary, rgba(255, 255, 255, 0.5));
	}

	.action.allow {
		background: var(--ca-accent, #4f8ef7);
		border-color: transparent;
		color: #fff;
	}

	.allowed-row {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
		font-weight: 500;
		color: var(--ca-semantic-success, #4ade80);
	}

	.allowed-dot {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: currentColor;
		flex-shrink: 0;
	}

	/* ── Phone ──────────────────────────────────────────────────────────── */

	.phone-wrap {
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.phone {
		background: var(--ca-surface-raised, rgba(255, 255, 255, 0.04));
		border: 1.5px solid var(--ca-border-subtle, rgba(255, 255, 255, 0.12));
		border-radius: 18px;
		display: flex;
		flex-direction: column;
		padding: 10px 8px 8px;
		gap: 6px;
		pointer-events: none;
		user-select: none;
		box-shadow: 0 4px 24px rgba(0, 0, 0, 0.35);
	}

	.phone-status {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0 4px;
		color: var(--ca-text-secondary, rgba(255, 255, 255, 0.4));
	}

	.phone-time {
		font-size: 10px;
		font-weight: 600;
	}

	/* Notification card on lock screen */
	.notif-card {
		background: rgba(255, 255, 255, 0.07);
		border: 1px solid rgba(255, 255, 255, 0.09);
		border-radius: 10px;
		padding: 8px 9px;
		display: flex;
		flex-direction: column;
		gap: 5px;
	}

	.notif-app {
		font-size: 9px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--ca-text-secondary, rgba(255, 255, 255, 0.4));
	}

	.notif-title {
		font-size: 11px;
		font-weight: 500;
		color: var(--ca-text-primary);
	}

	.notif-actions {
		display: flex;
		gap: 5px;
		justify-content: flex-end;
		margin-top: 2px;
	}

	.notif-btn {
		font-size: 9px;
		font-weight: 500;
		padding: 3px 8px;
		border-radius: 999px;
		background: rgba(255, 255, 255, 0.07);
		border: 1px solid rgba(255, 255, 255, 0.1);
		color: var(--ca-text-secondary, rgba(255, 255, 255, 0.55));
	}

	.notif-btn.allow {
		background: var(--ca-accent, #4f8ef7);
		border-color: transparent;
		color: #fff;
		position: relative;
	}

	/* Tap ring: expands from center of Allow button */
	.tap-ring {
		position: absolute;
		top: 50%;
		left: 50%;
		width: 100%;
		aspect-ratio: 1;
		border-radius: 50%;
		border: 1.5px solid rgba(79, 142, 247, 0.7);
		animation: tap-expand 600ms ease-out both;
		pointer-events: none;
	}

	@media (prefers-reduced-motion: reduce) {
		.tap-ring {
			display: none;
		}
	}

	/* ── Done bubble ─────────────────────────────────────────────────────── */

	.done-bubble {
		padding: 6px 18px;
		background: var(--ca-semantic-success-subtle, rgba(34, 197, 94, 0.12));
		border: 1px solid rgba(74, 222, 128, 0.28);
		border-radius: 999px;
		font-size: 12px;
		font-weight: 500;
		color: var(--ca-semantic-success, #4ade80);
		white-space: nowrap;
		pointer-events: none;
		user-select: none;
	}
</style>
