<script lang="ts">
	import { store } from "#lib/stores/projects.svelte.ts";
	import { escapeEvent, primaryEvent } from "./flow.ts";
	import Hero from "./Hero.svelte";
	import { onboarding } from "./state.svelte.ts";
	import { welcome } from "./welcome.svelte.ts";

	// A project showing up through any other path (CLI, the sidebar) ends the welcome and starts the tour, as before.
	// Paths through the hero itself walk the guided flow and hand off from its last step.
	let handedOff = false;
	// Captured before the flag below flips: this overlay instance decides once whether the intro plays.
	const seenAtMount = onboarding.state.introSeen;
	// The title sequence plays once per machine: the moment the overlay mounts it counts as seen, so a crash or a
	// quit mid-intro does not replay it next launch.
	$effect(() => {
		if (onboarding.loaded) void onboarding.markIntroSeen();
	});
	$effect(() => {
		if (!store.projects.length || handedOff || welcome.phase !== "welcome") return;
		handedOff = true;
		void finish();
	});

	async function finish() {
		welcome.send({ t: "skip" });
		await onboarding.finishWelcome();
		setTimeout(() => onboarding.startTour(), 600);
	}

	function skip() {
		welcome.send({ t: "skip" });
		void onboarding.finishWelcome();
	}

	/** Esc backs out one step; Enter runs the step's primary action unless a control already has focus. */
	function onkeydown(e: KeyboardEvent) {
		if (e.metaKey || e.ctrlKey || e.altKey || e.defaultPrevented) return;
		if (e.key === "Escape") {
			const ev = escapeEvent(welcome.phase);
			if (!ev) return;
			e.preventDefault();
			if (ev.t === "cancel") void welcome.cancelScan();
			else skip();
			return;
		}
		if (e.key !== "Enter" || (e.target as HTMLElement | null)?.closest("button, input, a, [role=checkbox]"))
			return;
		const act = primaryEvent(welcome.phase);
		if (!act) return;
		e.preventDefault();
		if (act === "scan") void welcome.scan();
		else if (act === "init") void welcome.initFolder();
		else if (act === "add") void welcome.addChosen();
		else void finish();
	}
</script>

<svelte:window {onkeydown} />

<div class="first" role="dialog" aria-label="Welcome to Atlas">
	<div class="drag" data-tauri-drag-region></div>
	<Hero intro seen={seenAtMount} onfinish={() => void finish()} onskip={skip} />
</div>

<style>
	.first {
		position: fixed;
		inset: 0;
		z-index: 80;
		/* Opaque: a full-window blur recomposites the transparent window on every frame beneath it. */
		background: var(--ca-surface-base);
		animation: fade 320ms cubic-bezier(0.2, 0.8, 0.2, 1);
	}
	.drag {
		position: absolute;
		inset: 0 0 auto 0;
		height: 40px;
		z-index: 3;
	}
	@keyframes fade {
		from {
			opacity: 0;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.first {
			animation: none;
		}
	}
</style>
