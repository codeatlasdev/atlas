<script lang="ts">
	import { kbd } from "#lib/keys.ts";
	/**
	 * The welcome screen: one sky, one action per phase. Used full-screen on first run and as the standing empty
	 * state when there is no project. The copy and the controls change with the phase; the sky stays.
	 */
	import { Button, Checkbox, Kbd, Spinner } from "@codeatlasdev/ui";
	import { ArrowRight, Check, FolderPlus, Sparkles, Wand2 } from "@lucide/svelte";
	import { tilde } from "#lib/format.ts";
	import { ago } from "#lib/components/sidebar/discovery.ts";
	import { discovery } from "#lib/stores/discovery.svelte.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { ACTIONS, testId } from "./flow.ts";
	import { copyScrim, STAGE_HEX, STAGE_LAYOUT, STAGE_RGB } from "./legibility.ts";
	import Globe from "./intro/Globe.svelte";
	import { welcome as w } from "./welcome.svelte.ts";
	import { themes } from "#lib/stores/theme.svelte.ts";
	import { cssVars } from "#lib/theme.ts";

	// The stage is always a night sky, whatever the app theme: everything on it (copy, buttons, chips) uses the
	// user's DARK theme of the same family, so light themes stay readable here (2026-10-13 light-theme incident).
	const darkVars = $derived(
		themes.currentDark
			? Object.entries(cssVars(themes.currentDark.style))
					.map(([k, v]) => `${k}:${v}`)
					.join(";")
			: "",
	);

	let {
		onfinish,
		onskip,
		intro = false,
		seen = true,
	}: {
		onfinish: () => void;
		onskip?: () => void;
		/** First run: play the title sequence before the copy. */
		intro?: boolean;
		/** The intro already played on this machine. */
		seen?: boolean;
	} = $props();

	/**
	 * The scrim under the copy. Its strength comes from the brightest pixel the globe can put behind the subtitle, measured
	 * on a real shot of the un-dimmed globe ([125, 143, 220] on the tiles' light side), so it holds even if the globe is
	 * ever moved back under the text; the subtitle colour is --ca-text-secondary on the dark stage.
	 */
	const scrim = copyScrim({ worstBackdrop: [125, 143, 220], subtitle: [166, 168, 171] });

	let skip = $state(0);
	let globe: ReturnType<typeof Globe> | undefined = $state();
	// Without the intro (the standing empty state, or one already seen) the copy is there from the start.
	let showCopy = $derived(!intro || w.introDone);
	let mode = $derived<"intro" | "progress" | "calm">(
		intro && !w.introDone ? "intro" : w.phase === "welcome" && !intro ? "calm" : "progress",
	);

	$effect(() => {
		w.clock = () => globe?.now?.() ?? 0;
	});

	/** Any key or click during the title sequence skips it. */
	function skipIntro() {
		if (mode === "intro") skip++;
	}

	let names = $derived(
		w.summary.top
			.map((f) => f.name)
			.concat(discovery.found.filter((f) => f.has_config && !f.nested).map((f) => f.name)),
	);
	let live = $derived(store.live[w.watching ?? ""]?.services ?? []);

	// Watch the stack we started; the machine decides when it is up or has failed.
	$effect(() => {
		void w.started;
		w.observe();
	});

	const plural = (n: number, one: string, many = `${one}s`) =>
		`${n.toLocaleString()} ${n === 1 ? one : many}`;
</script>

<svelte:window onkeydown={skipIntro} />

<div
	class="hero dark"
	style={darkVars}
	data-contrast-bg={STAGE_HEX}
	data-phase={w.phase}
	data-testid={testId(w.phase)}
	data-intro={mode === "intro" ? "playing" : "done"}
	onpointerdown={skipIntro}
	role="presentation"
>
	<Globe
		bind:this={globe}
		{mode}
		{seen}
		lit={w.summary.projects}
		doneAt={w.upAt}
		skipSignal={skip}
		ondone={() => (w.introDone = true)}
	/>

	<div
		class="scrim"
		class:hidden={!showCopy}
		style:--scrim={scrim.alpha}
		style:--inset="{STAGE_LAYOUT.copyBottomInset}px"
		style:--stage-rgb={STAGE_RGB.join(", ")}
		aria-hidden="true"
	></div>
	<div
		class="stage"
		class:hidden={!showCopy}
		class:anchored={showCopy}
		style:--inset="{STAGE_LAYOUT.copyBottomInset}px"
	>
		{#if w.phase === "welcome"}
			{@render welcome()}
		{:else if w.phase === "scanning"}
			{@render scanning()}
		{:else if w.phase === "empty"}
			{@render empty()}
		{:else if w.phase === "results"}
			{@render results()}
		{:else if w.phase === "starting"}
			{@render starting()}
		{:else if w.phase === "up"}
			{@render up()}
		{/if}
	</div>

	{#if onskip && w.phase !== "up" && showCopy}
		<button class="skip" data-testid={ACTIONS.skip} onclick={onskip}>Skip for now</button>
	{/if}
</div>

{#snippet welcome()}
	<h1>Everything you run,<br />in one place.</h1>
	<p class="lead">Atlas finds the projects already on this computer and starts them together.</p>
	<div class="actions">
		<Button size="lg" data-testid={ACTIONS.scan} onclick={() => void w.scan()}>
			{#snippet leading()}<Sparkles size={15} />{/snippet}
			Find my projects
		</Button>
		<div class="secondary">
			<button class="link" data-testid={ACTIONS.addFolder} onclick={() => void w.addFolder()}
				><FolderPlus size={13} /> Add a folder…</button
			>
			<button class="link" onclick={() => void w.initFolder()}
				><Wand2 size={13} /> Set up a folder with atlas init</button
			>
		</div>
	</div>
{/snippet}

{#snippet scanning()}
	<h1 aria-live="polite">
		<span class="count">{w.summary.projects.toLocaleString()}</span>
		{w.summary.projects === 1 ? "project" : "projects"} so far
	</h1>
	<p class="lead">
		<Spinner size={12} />
		{discovery.dirs.toLocaleString()} folders · {(discovery.elapsedMs / 1000).toFixed(1)}s
	</p>
	<ul class="live" aria-label="Projects found">
		{#each w.summary.top.slice(0, 5) as f (f.path)}
			<li>
				<span class="dot"></span>
				<b>{f.name}</b>
				<span class="meta">{plural(f.services, "service")}</span>
			</li>
		{/each}
	</ul>
	<p class="cur mono" title={discovery.current}>{tilde(discovery.current)}</p>
	<div class="actions">
		<Button variant="ghost" onclick={() => void w.cancelScan()}>Stop</Button>
	</div>
{/snippet}

{#snippet empty()}
	<h1>No atlas.yaml found yet.</h1>
	<p class="lead">
		Scanned {discovery.done ? discovery.done.dirs.toLocaleString() : 0} folders. Point Atlas at a project and it
		sets one up for you.
	</p>
	<div class="actions">
		<Button size="lg" data-testid={ACTIONS.init} onclick={() => void w.initFolder()}>
			{#snippet leading()}<Wand2 size={15} />{/snippet}
			Set up a folder with atlas init
		</Button>
		<div class="secondary">
			<button class="link" data-testid={ACTIONS.addFolder} onclick={() => void w.addFolder()}
				><FolderPlus size={13} /> Add a folder…</button
			>
			<button class="link" onclick={() => void w.scan()}>Scan again</button>
		</div>
	</div>
{/snippet}

{#snippet results()}
	<h1>
		Found <span class="count">{plural(w.summary.projects, "project")}</span>
	</h1>
	<p class="lead">
		{plural(w.summary.services, "service")} ready to start{w.summary.broken
			? `, ${plural(w.summary.broken, "config")} need a look`
			: ""}.
	</p>
	<ul class="pick" aria-label="Projects to add">
		{#each w.choices.slice(0, 7) as f (f.path)}
			<li>
				<Checkbox
					checked={w.picked[f.path] ?? true}
					onchange={() => w.toggle(f.path)}
					aria-label="Add {f.name}"
				/>
				<span class="name"><b>{f.name}</b><span class="meta mono">{tilde(f.path)}</span></span>
				<span class="meta">{plural(f.services, "service")} · {ago(f.modified)}</span>
			</li>
		{/each}
	</ul>
	{#if w.choices.length > 7}<p class="more">and {w.choices.length - 7} more</p>{/if}
	<div class="actions">
		<Button
			size="lg"
			data-testid={ACTIONS.add}
			disabled={!w.chosen.length}
			onclick={() => void w.addChosen()}
		>
			Add {w.chosen.length === 1 ? "this project" : `${w.chosen.length} projects`} and start
			{#snippet trailing()}<ArrowRight size={15} />{/snippet}
		</Button>
		<div class="secondary">
			<button class="link" onclick={() => void w.scan()}>Scan again</button>
		</div>
	</div>
{/snippet}

{#snippet starting()}
	<h1>Starting your stack…</h1>
	<p class="lead tabular">{w.started.up} of {w.started.total || "…"} services up</p>
	<ul class="services" aria-label="Services">
		{#each live.filter((s) => !s.deselected && !s.completed_ok) as s (s.name)}
			<li class={s.state}>
				<span class="led" aria-hidden="true"></span>
				<b>{s.name}</b>
				<span class="meta">{s.state}</span>
			</li>
		{/each}
	</ul>
{/snippet}

{#snippet up()}
	<div class="check" aria-hidden="true"><Check size={26} /></div>
	<h1>Your stack is up.</h1>
	<p class="lead">
		{plural(w.started.total, "service")} running. Press <Kbd>{kbd("cmd-k")}</Kbd> to jump anywhere.
	</p>
	<div class="actions">
		<Button size="lg" data-testid={ACTIONS.finish} onclick={onfinish}>
			Take a 30-second look around
			{#snippet trailing()}<ArrowRight size={15} />{/snippet}
		</Button>
	</div>
{/snippet}

<style>
	.hero {
		position: absolute;
		inset: 0;
		display: grid;
		place-items: center;
		overflow: hidden;
		isolation: isolate;
		/* Inherited text (the h1) must come from the dark vars set on this element, not the app body. */
		color: var(--ca-text-primary);
	}
	/* Settled: the copy block sits at the bottom of the hero so the globe above it never meets a letter (legibility.ts). */
	.stage.anchored {
		position: absolute;
		left: 50%;
		bottom: var(--inset);
		/* `translate` is its own property: the entrance animation owns `transform`, so the two compose instead of fighting. */
		translate: -50% 0;
	}
	.stage.hidden,
	.scrim.hidden {
		opacity: 0;
		pointer-events: none;
	}
	/* A soft pool of the stage colour under the copy: opaque enough for AA at its centre, feathered so it never reads as a card. */
	.scrim {
		position: absolute;
		z-index: 0;
		left: 50%;
		bottom: calc(var(--inset) - 90px);
		width: min(640px, 92%);
		height: 420px;
		translate: -50% 0;
		pointer-events: none;
		background: radial-gradient(
			ellipse 50% 50% at 50% 50%,
			rgba(var(--stage-rgb), var(--scrim)) 0%,
			rgba(var(--stage-rgb), calc(var(--scrim) * 0.9)) 55%,
			rgba(var(--stage-rgb), 0) 100%
		);
		transition: opacity 400ms cubic-bezier(0.2, 0.8, 0.2, 1);
	}
	.stage {
		position: relative;
		z-index: 1;
		width: min(460px, calc(100% - 56px));
		display: flex;
		flex-direction: column;
		align-items: center;
		text-align: center;
		gap: 10px;
		animation: land 520ms cubic-bezier(0.2, 0.8, 0.2, 1) both;
	}
	/* Each phase re-lands: the copy changes, the sky does not. */
	.hero[data-phase] .stage {
		animation-name: land;
	}
	h1 {
		margin: 0;
		font-size: 34px;
		line-height: 1.08;
		font-weight: 720;
		letter-spacing: -0.035em;
		text-wrap: balance;
	}
	.count {
		font-variant-numeric: tabular-nums;
		color: var(--ca-brand-main);
	}
	.lead {
		margin: 0;
		display: inline-flex;
		align-items: center;
		gap: 7px;
		font-size: 14px;
		color: var(--ca-text-secondary);
		max-width: 38ch;
		text-wrap: pretty;
	}
	.tabular {
		font-variant-numeric: tabular-nums;
	}
	.actions {
		margin-top: 14px;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 10px;
	}
	.secondary {
		display: flex;
		gap: 14px;
		flex-wrap: wrap;
		justify-content: center;
	}
	.link {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		border: 0;
		background: none;
		font: inherit;
		font-size: 12.5px;
		color: var(--ca-text-secondary);
		padding: 5px 8px;
		border-radius: 8px;
	}
	.link:hover {
		color: var(--ca-text-primary);
		background: var(--ca-action-hover);
	}
	ul {
		list-style: none;
		margin: 8px 0 0;
		padding: 0;
		width: 100%;
		display: flex;
		flex-direction: column;
		gap: 4px;
		text-align: left;
	}
	li {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 8px 12px;
		border-radius: 10px;
		background: color-mix(in oklab, var(--ca-surface-raised) 86%, transparent);
		box-shadow: 0 0 0 0.5px var(--ca-outline-subtle);
		font-size: 13px;
		animation: rise 320ms cubic-bezier(0.2, 0.8, 0.2, 1) backwards;
	}
	li b {
		font-weight: 600;
	}
	.name {
		display: flex;
		flex-direction: column;
		min-width: 0;
		flex: 1;
	}
	.meta {
		margin-left: auto;
		font-size: 11.5px;
		color: var(--ca-text-secondary);
		white-space: nowrap;
	}
	.name .meta {
		margin: 0;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.dot,
	.led {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--ca-brand-main);
		box-shadow: 0 0 10px var(--ca-brand-main);
		flex: none;
	}
	.services li .led {
		background: var(--ca-text-disabled);
		box-shadow: none;
		transition:
			background 260ms,
			box-shadow 260ms;
	}
	.services li.starting .led {
		background: var(--ca-warning-main, #e5a50a);
		animation: breathe 1s ease-in-out infinite;
	}
	.services li.running .led {
		background: var(--ca-success-main, #2fbf71);
		box-shadow: 0 0 10px var(--ca-success-main, #2fbf71);
	}
	.services li.failed .led {
		background: var(--ca-error-main, #e5484d);
	}
	.cur {
		margin: 4px 0 0;
		max-width: 100%;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: 11px;
		color: var(--ca-text-secondary);
	}
	.more {
		margin: 2px 0 0;
		font-size: 11.5px;
		color: var(--ca-text-secondary);
	}
	.mono {
		font-family: var(--mono);
	}
	.check {
		width: 56px;
		height: 56px;
		border-radius: 50%;
		display: grid;
		place-items: center;
		color: var(--ca-success-main, #2fbf71);
		background: color-mix(in oklab, var(--ca-success-main, #2fbf71) 16%, transparent);
		box-shadow: 0 0 0 1px color-mix(in oklab, var(--ca-success-main, #2fbf71) 40%, transparent);
		animation: pop 560ms cubic-bezier(0.2, 1.3, 0.3, 1) both;
	}
	.skip {
		position: absolute;
		right: 20px;
		bottom: 18px;
		z-index: 2;
		border: 0;
		background: none;
		font: inherit;
		font-size: 12px;
		color: var(--ca-text-secondary);
		padding: 5px 8px;
		border-radius: 7px;
	}
	.skip:hover {
		color: var(--ca-text-secondary);
		background: var(--ca-action-hover);
	}
	@keyframes land {
		from {
			opacity: 0;
			transform: translateY(10px);
		}
	}
	@keyframes rise {
		from {
			opacity: 0;
			transform: translateY(6px);
		}
	}
	@keyframes pop {
		from {
			opacity: 0;
			transform: scale(0.6);
		}
	}
	@keyframes breathe {
		50% {
			opacity: 0.4;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.stage,
		li,
		.check,
		.services li.starting .led {
			animation: none;
		}
	}
</style>
