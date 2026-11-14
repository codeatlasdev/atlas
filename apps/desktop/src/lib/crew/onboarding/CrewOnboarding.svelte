<!-- Atlas Crew first-run onboarding shell: overlay, progress bar, step transitions and the two panels. The flow lives in onboarding.svelte.ts. -->
<script lang="ts">
	import { cubicOut, cubicIn } from "svelte/easing";
	import type { TransitionConfig } from "svelte/transition";
	import { ChevronLeft } from "@lucide/svelte";
	import CrewArtIntro from "#lib/crew/CrewArtIntro.svelte";
	import CrewArtHow from "#lib/crew/CrewArtHow.svelte";
	import { reducedMotion } from "#lib/crew/motion.ts";
	import ConnectStep from "./ConnectStep.svelte";
	import ConnectActions from "./ConnectActions.svelte";
	import InstallChecklist from "./InstallChecklist.svelte";
	import EngineStep from "./EngineStep.svelte";
	import EngineActions from "./EngineActions.svelte";
	import EngineSignIn from "./EngineSignIn.svelte";
	import EngineArt from "./EngineArt.svelte";
	import ReadyHero from "./ReadyHero.svelte";
	import { STORY } from "./engines.ts";
	import { Onboarding } from "./onboarding.svelte.ts";
	import "./onboarding.css";

	let { ondone }: { ondone: () => void } = $props();

	// The flow reads `ondone` once at construction; the parent passes a stable callback.
	// svelte-ignore state_referenced_locally
	const ob = new Onboarding(ondone);

	// Effects that follow the flow state: detect the SSH key, promote an old root@host server, check the engines,
	// and stream the sign-in terminal.
	$effect(() => ob.checkSshKey());
	$effect(() => ob.promoteOldServer());
	$effect(() => ob.checkEngines());
	$effect(() => ob.watchSigninPty());

	// ─── parallax transitions ─────────────────────────────────────────────────

	function textIn(_node: Element, { d }: { d: number }): TransitionConfig {
		return {
			duration: reducedMotion ? 0 : 380,
			easing: cubicOut,
			// Only translateX — children stagger opacity via CSS animation
			css: (t) => `transform: translateX(${(1 - t) * -d * 64}px)`,
		};
	}

	function textOut(_node: Element, { d }: { d: number }): TransitionConfig {
		return {
			duration: reducedMotion ? 0 : 200,
			easing: cubicIn,
			css: (t) => `opacity: ${t}; transform: translateX(${(1 - t) * d * 64}px)`,
		};
	}

	function artIn(_node: Element, { d }: { d: number }): TransitionConfig {
		return {
			duration: reducedMotion ? 0 : 400,
			easing: cubicOut,
			css: (t) => `opacity: ${t}; transform: translateX(${(1 - t) * d * 220}px) scale(${0.92 + 0.08 * t})`,
		};
	}

	function artOut(_node: Element, { d }: { d: number }): TransitionConfig {
		return {
			duration: reducedMotion ? 0 : 200,
			easing: cubicIn,
			css: (t) => `opacity: ${t}; transform: translateX(${(1 - t) * -d * 220}px) scale(${0.92 + 0.08 * t})`,
		};
	}
</script>

<div
	class="crew-onboarding overlay"
	role="dialog"
	aria-modal="true"
	aria-label="Atlas Crew setup"
	data-testid="crew-onboarding"
>
	<!-- Progress bar + Skip -->
	<div class="top-bar">
		<div class="progress-track" role="progressbar" aria-valuenow={Math.round(ob.progress * 100)}>
			<div class="progress-fill" style:width="{ob.progress * 100}%"></div>
		</div>
		<button class="skip-btn" onclick={ob.finish}>Skip</button>
	</div>

	<!-- Two-column body -->
	<div class="body">
		<!-- ─── LEFT: text content ─────────────────────────────────────────── -->
		<div class="left-panel">
			{#key ob.step}
				<div class="text-block" in:textIn={{ d: ob.direction }} out:textOut={{ d: ob.direction }}>
					<!-- Info step label "N of 2" -->
					{#if ob.isInfoStep}
						<span class="step-label" style:--stagger={0}>{ob.meta.label}</span>
					{:else if ob.step === "connect" || ob.step === "engine"}
						<span class="step-label setup" style:--stagger={0}>Setup</span>
					{/if}

					<h1 class="headline" style:--stagger={1}>{ob.meta.headline}</h1>
					<p class="body-text" style:--stagger={2}>{ob.meta.body}</p>

					<!-- Bullet points (info steps only) -->
					{#if ob.meta.bullets?.length}
						<ul class="bullets" style:--stagger={3}>
							{#each ob.meta.bullets as bullet}
								<li>{bullet}</li>
							{/each}
						</ul>
					{/if}

					<!-- The how-it-works story, lit in sync with the art -->
					{#if ob.step === "how"}
						<ol class="story" style:--stagger={3}>
							{#each STORY as line, i (line.title)}
								<li class:on={ob.howBeat === i} class:past={ob.howBeat > i}>
									<b>{line.title}</b>
									<span>{line.text}</span>
								</li>
							{/each}
						</ol>
					{/if}

					{#if ob.step === "connect"}
						<ConnectStep {ob} />
					{:else if ob.step === "engine"}
						<EngineStep {ob} />
					{/if}

					<!-- ─── Actions row ────────────────────────────────────────── -->
					<div class="actions" style:--stagger={ob.meta.bullets?.length ? 4 : 3}>
						<!-- Back button (not on intro) -->
						{#if ob.step !== "intro" && ob.step !== "ready"}
							<button class="back-btn" onclick={ob.back}>
								<ChevronLeft size={15} />Back
							</button>
						{/if}

						{#if ob.step === "intro" || ob.isInfoStep}
							<button class="primary-btn" onclick={ob.advance}>Continue</button>
						{:else if ob.step === "connect"}
							<ConnectActions {ob} />
						{:else if ob.step === "engine"}
							<EngineActions {ob} />
						{:else if ob.step === "ready"}
							<button
								class="primary-btn"
								onclick={() => ob.openChat()}
								disabled={ob.creatingCrewmate || !ob.engineReady}
							>
								{ob.creatingCrewmate ? "Opening…" : "Open chat"}
							</button>
							<button class="back-btn" onclick={ob.back}>
								<ChevronLeft size={15} />Back
							</button>
							{#if !ob.engineReady && !ob.creatingCrewmate}
								<!-- A disabled button with no reason looked like "nothing happens". -->
								<p class="field-hint">
									{ob.selectedEngine
										? `${ob.selectedEngine} isn't ready on your crew server yet (${ob.engineStatus[ob.selectedEngine]?.phase ?? "not checked"}). Go back and finish it.`
										: "Go back and pick an engine."}
								</p>
							{/if}
						{/if}
					</div>
				</div>
			{/key}
		</div>

		<!-- ─── RIGHT: illustration ──────────────────────────────────────── -->
		<div class="right-panel">
			{#key ob.step}
				<div
					class="art-zone"
					style:--art-glow={ob.meta.glow}
					in:artIn={{ d: ob.direction }}
					out:artOut={{ d: ob.direction }}
				>
					{#if ob.step === "intro"}
						<CrewArtIntro />
					{:else if ob.step === "how"}
						<CrewArtHow bind:beat={ob.howBeat} />
					{:else if ob.step === "connect"}
						<InstallChecklist done={ob.install.done} active={ob.install.active} failed={ob.failedPhase} />
					{:else if ob.step === "engine"}
						{#if ob.signinActive && (ob.signinPtyId || ob.apiKeyMode || ob.providerOpen)}
							<EngineSignIn {ob} />
						{:else}
							<EngineArt {ob} />
						{/if}
					{:else if ob.step === "ready"}
						<ReadyHero engine={ob.selectedEngine} look={ob.previewLook} />
					{/if}
				</div>
			{/key}
		</div>
	</div>
</div>

<style>
	.overlay {
		position: fixed;
		inset: 0;
		z-index: 1600;
		background: var(--ca-surface-base);
		display: flex;
		flex-direction: column;
		min-width: 600px;
	}

	/* ─── Top bar ────────────────────────────────────────────────── */
	.top-bar {
		flex-shrink: 0;
		display: flex;
		align-items: center;
		gap: 0;
		height: 36px;
		/* macOS: the traffic lights sit over the top ~40px of the window; keep the bar clear of them. */
		margin-top: 40px;
	}
	:global(html.opaque-window) .top-bar {
		margin-top: 0;
	}

	.progress-track {
		flex: 1;
		height: 3px;
		background: var(--ca-surface-sunken);
	}

	.progress-fill {
		height: 100%;
		background: var(--ca-accent, #4f8ef7);
		transition: width 400ms ease;
	}

	.skip-btn {
		flex-shrink: 0;
		padding: 8px 20px;
		background: none;
		border: none;
		color: var(--ca-text-secondary);
		font-size: 13px;
		cursor: pointer;
		border-radius: 0;
		transition: color 140ms;
	}

	.skip-btn:hover {
		color: var(--ca-text-primary);
	}

	/* ─── Body columns ───────────────────────────────────────────── */
	.body {
		flex: 1;
		display: grid;
		grid-template-columns: 45fr 55fr;
		min-height: 0;
		overflow: hidden;
	}

	/* ─── Left panel ─────────────────────────────────────────────── */
	.left-panel {
		position: relative;
		display: grid;
		align-content: center;
		overflow: hidden;
		/* Everything inside scales with the column (cqi), so a full-screen window gets a full-size page,
		   not the laptop layout floating in a corner. */
		container-type: inline-size;
		padding: 64px clamp(32px, 6cqi, 96px) 64px clamp(48px, 9cqi, 140px);
	}

	/* entering + exiting text-blocks both live in cell 1/1 — no width competition */
	.left-panel > * {
		grid-area: 1 / 1;
		min-width: 0;
	}

	.text-block {
		width: 100%;
		max-width: clamp(420px, 62cqi, 760px);
		display: flex;
		flex-direction: column;
		gap: 0;
	}

	/* Stagger animation: each direct child (the step's components included) fades + lifts in */
	.text-block > :global(*) {
		animation: child-in 320ms calc(var(--stagger, 0) * 60ms) both;
	}

	@keyframes child-in {
		from {
			opacity: 0;
			transform: translateY(6px);
		}
		to {
			opacity: 1;
			transform: none;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.text-block > :global(*) {
			animation: none;
		}
	}

	.step-label {
		display: block;
		font-size: clamp(12px, 1.25cqi, 16px);
		font-weight: 600;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--ca-accent, #4f8ef7);
		margin-bottom: clamp(12px, 1.6cqi, 22px);
	}

	.step-label.setup {
		color: var(--ca-text-secondary);
	}

	.headline {
		font-size: clamp(34px, 5.4cqi, 76px);
		font-weight: 700;
		line-height: 1.05;
		letter-spacing: -0.03em;
		color: var(--ca-text-primary);
		margin: 0 0 clamp(16px, 2.2cqi, 32px);
	}

	.body-text {
		font-size: clamp(16px, 1.9cqi, 26px);
		line-height: 1.5;
		color: var(--ca-text-secondary);
		margin: 0 0 clamp(20px, 3cqi, 44px);
		max-width: 34em;
	}

	.story {
		list-style: none;
		margin: 20px 0 36px;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.story li {
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding: 10px 14px;
		border-left: 2px solid var(--ca-border);
		opacity: 0.45;
		transition:
			opacity 400ms ease,
			border-color 400ms ease,
			background 400ms ease;
	}
	.story li.past {
		opacity: 0.75;
	}
	.story li.on {
		opacity: 1;
		border-left-color: #3d5bff;
		background: rgb(61 91 255 / 0.08);
	}
	.story b {
		font-size: 15px;
	}
	.story span {
		font-size: 13px;
		color: var(--ca-text-secondary);
	}

	.bullets {
		list-style: none;
		padding: 0;
		margin: 0 0 clamp(28px, 4cqi, 56px);
		display: flex;
		flex-direction: column;
		gap: clamp(10px, 1.3cqi, 18px);
	}

	.bullets li {
		font-size: clamp(14px, 1.55cqi, 21px);
		color: var(--ca-text-secondary);
		display: flex;
		align-items: flex-start;
		gap: 10px;
		line-height: 1.4;
	}

	.bullets li::before {
		content: "";
		flex-shrink: 0;
		width: 0.35em;
		height: 0.35em;
		border-radius: 50%;
		background: var(--ca-accent, #4f8ef7);
		margin-top: 0.5em;
		opacity: 0.7;
	}

	.actions {
		display: flex;
		align-items: center;
		gap: 12px;
		flex-wrap: wrap;
	}

	/* ─── Right panel ────────────────────────────────────────────── */
	.right-panel {
		position: relative;
		overflow: hidden;
		background: var(--ca-surface-sunken);
		display: grid;
		place-items: center;
	}

	/* entering + exiting art-zones both live in cell 1/1 */
	.right-panel > * {
		grid-area: 1 / 1;
		width: 100%;
		height: 100%;
	}

	.art-zone {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		/* Per-step accent glow */
		background: radial-gradient(ellipse 80% 70% at 50% 55%, var(--art-glow, transparent), transparent 75%);
	}
</style>
