<!--
  The art of the connect step, the hero of its panel: this computer -> SSH link -> the server, lit phase by phase while
  the install runs. Idle: the link breathes and the cursor blinks. Active: a packet (a key, a probe, a package, a badge,
  a bolt) travels the link in the accent colour. Done: green. Failed: the link breaks red on the server's side and the
  rack turns red. Only transform and opacity move; colours are the onboarding tokens of onboarding.css.
-->
<script lang="ts">
	import { reducedMotion } from "#lib/crew/motion.ts";
	import type { InstallPhase } from "#lib/crew/installSteps.ts";
	import { machineName } from "./machine.ts";
	import ServerRack from "./ServerRack.svelte";
	import { m } from "#lib/i18n/m.ts";

	let {
		done,
		active,
		failed,
	}: { done: Set<InstallPhase>; active: InstallPhase | null; failed: InstallPhase | null } = $props();

	const machine = $derived(machineName());
	const lit = (p: InstallPhase) => done.has(p) || active === p;
	const live = $derived(done.has("done"));
	const rack = $derived(failed ? "failed" : live ? "ok" : active ? "active" : "idle");
	const running = $derived(active !== null && failed === null && !live);
</script>

<div
	class="scene"
	class:still={reducedMotion}
	data-testid="connect-scene"
	data-active={active ?? ""}
	data-failed={failed ?? ""}
	data-state={rack}
>
	<svg
		viewBox="6 56 628 250"
		role="img"
		aria-label={m.crew_onboarding_connect_scene_machine_connecting({ machine })}
		preserveAspectRatio="xMidYMid meet"
	>
		<!-- this computer: a screen with a terminal on it -->
		<g class="me" class:lit={running || live}>
			<rect class="screen" x="30" y="86" width="200" height="130" rx="12" />
			<rect class="glass" x="40" y="96" width="180" height="110" rx="6" />
			<path class="txt" d="M54 118h46M54 134h96M54 150h70M54 166h112" />
			<!-- i18n-ignore: a command, not prose -->
			<text class="prompt" x="54" y="192">$ atlas crew install</text>
			<rect class="cursor" x="170" y="182" width="8" height="13" rx="1.5" />
			<path class="base" d="M30 216h200l18 18a6 6 0 0 1-4 10H16a6 6 0 0 1-4-10z" />
			<text class="cap" x="130" y="266" text-anchor="middle">{machine}</text>
		</g>

		<!-- the link -->
		<line class="link" x1="250" y1="150" x2="440" y2="150" />
		<line class="link flow" class:on={running} x1="250" y1="150" x2="440" y2="150" />
		<line class="link bad" class:on={failed !== null} x1="345" y1="150" x2="440" y2="150" />
		<line class="link good" class:on={live} x1="250" y1="150" x2="440" y2="150" />
		{#if failed}
			<g class="x" transform="translate(440 150)">
				<circle r="13" />
				<path d="M-5 -5l10 10M5 -5l-10 10" />
			</g>
		{/if}

		<!-- the packet that crosses the link while a phase runs: its glyph is the phase's -->
		{#if running}
			{#key active}
				<g class="packet p-{active}" transform="translate(250 150)">
					<g class="glyph">
						<circle class="halo" r="20" />
						{#if active === "signin" || active === "ssh"}
							<circle class="g" cx="-6" r="6" />
							<path class="g" d="M0 0h16M11 0v6M15 0v5" />
						{:else if active === "check"}
							<circle class="g" r="7" />
							<circle class="g ring" r="13" />
						{:else if active === "install"}
							<rect class="g" x="-9" y="-9" width="18" height="18" rx="3" />
							<path class="g" d="M-9 -2h18M0 -9v7" />
						{:else if active === "user"}
							<circle class="g" cy="-4" r="5" />
							<path class="g" d="M-9 9c1-8 17-8 18 0" />
						{:else if active === "service"}
							<path class="g" d="M3 -11l-10 13h8l-3 11 11-14h-8z" />
						{:else}
							<path class="g" d="M-8 0l5 5 11-11" />
						{/if}
					</g>
				</g>
			{/key}
		{/if}

		<!-- the server -->
		<g class="srv" transform="translate(450 62)">
			<ServerRack state={rack} />
		</g>
		<text class="cap" x="525" y="266" text-anchor="middle"
			>{m.crew_onboarding_connect_scene_your_server()}</text
		>

		<!-- the key lands in authorized_keys -->
		<g class="auth" class:lit={lit("ssh")} class:go={active === "ssh"}>
			<rect x="446" y="284" width="158" height="26" rx="8" />
			<!-- i18n-ignore: a file name -->
			<text x="525" y="302" text-anchor="middle">authorized_keys</text>
		</g>
		<g transform="translate(560 30)">
			<g class="svc" class:lit={lit("service")} class:go={active === "service"}>
				<path d="M4 -12l-10 14h8l-3 12 12-15h-8z" />
			</g>
		</g>
		<g transform="translate(490 30)">
			<g class="usr" class:lit={lit("user")} class:go={active === "user"}>
				<circle cy="-3" r="5.5" />
				<path d="M-10 11c1-9 19-9 20 0" />
			</g>
		</g>
	</svg>
</div>

<style>
	.scene {
		width: 100%;
		min-height: 0;
		flex: 1 1 auto;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	svg {
		width: 100%;
		height: 100%;
		max-height: 100%;
		overflow: visible;
	}

	.screen {
		fill: var(--ob-panel, var(--ca-surface-base));
		stroke: var(--ob-line-idle, var(--ca-border-subtle));
		stroke-width: 2;
		transition: stroke 300ms;
	}

	.me.lit .screen {
		stroke: var(--ob-accent, var(--ca-accent));
	}

	.glass {
		fill: var(--ob-screen, var(--ca-surface-sunken));
	}

	.txt {
		stroke: var(--ob-line-idle, var(--ca-border-subtle));
		stroke-width: 5;
		stroke-linecap: round;
	}

	.prompt {
		fill: var(--ob-accent, var(--ca-accent));
		font:
			11px ui-monospace,
			monospace;
	}

	.cursor {
		fill: var(--ob-accent, var(--ca-accent));
		animation: ob-blink 1100ms steps(2, start) infinite;
	}

	.base {
		fill: var(--ob-panel, var(--ca-surface-base));
		stroke: var(--ob-line-idle, var(--ca-border-subtle));
		stroke-width: 2;
		stroke-linejoin: round;
		transition: stroke 300ms;
	}

	.me.lit .base {
		stroke: var(--ob-accent, var(--ca-accent));
	}

	.cap {
		fill: var(--ca-text-secondary);
		font-size: 15px;
	}

	.link {
		stroke: var(--ob-line-idle, var(--ca-border-subtle));
		stroke-width: 4;
		stroke-linecap: round;
		stroke-dasharray: 2 10;
	}

	.link.flow,
	.link.bad,
	.link.good {
		opacity: 0;
		transition: opacity 300ms;
		stroke-dasharray: none;
	}

	.link.flow {
		stroke: var(--ob-accent, var(--ca-accent));
		stroke-dasharray: 10 14;
	}

	.link.flow.on {
		opacity: 1;
		animation: ob-dash 900ms linear infinite;
	}

	.link.bad {
		stroke: var(--ob-danger, var(--ca-danger));
	}

	.link.good {
		stroke: var(--ob-ok, var(--ca-semantic-success));
	}

	.link.on {
		opacity: 1;
	}

	.x circle {
		fill: var(--ob-danger, var(--ca-danger));
	}

	.x path {
		stroke: var(--ca-surface-base);
		stroke-width: 3;
		stroke-linecap: round;
	}

	/* the packet travels once per cycle: transform only */
	.packet {
		animation: ob-travel 1500ms ease-in-out infinite;
	}

	.packet .halo {
		fill: var(--ob-glow, color-mix(in srgb, var(--ca-accent) 30%, transparent));
	}

	.packet .g {
		fill: none;
		stroke: var(--ob-accent, var(--ca-accent));
		stroke-width: 3;
		stroke-linecap: round;
		stroke-linejoin: round;
	}

	.packet.p-check .g,
	.packet.p-service .g {
		stroke: var(--ob-warm, var(--ca-semantic-warning));
	}

	.packet.p-check .halo,
	.packet.p-service .halo {
		fill: color-mix(in srgb, var(--ob-warm, var(--ca-semantic-warning)) 28%, transparent);
	}

	.packet .ring {
		animation: ob-ring 1500ms ease-out infinite;
		transform-origin: 0 0;
	}

	@keyframes ob-travel {
		from {
			transform: translate(250px, 150px);
		}
		to {
			transform: translate(440px, 150px);
		}
	}

	@keyframes ob-ring {
		from {
			transform: scale(0.6);
			opacity: 1;
		}
		to {
			transform: scale(1.5);
			opacity: 0;
		}
	}

	@keyframes ob-dash {
		to {
			stroke-dashoffset: -24;
		}
	}

	@keyframes ob-blink {
		to {
			opacity: 0;
		}
	}

	.auth rect,
	.svc,
	.usr {
		opacity: 0.25;
		transition: opacity 300ms;
	}

	.auth rect {
		fill: var(--ob-panel, var(--ca-surface-base));
		stroke: var(--ob-line-idle, var(--ca-border-subtle));
		stroke-width: 2;
	}

	.auth text {
		fill: var(--ca-text-secondary);
		font:
			12.5px ui-monospace,
			monospace;
	}

	.auth.lit rect {
		opacity: 1;
		stroke: var(--ob-ok, var(--ca-semantic-success));
	}

	.auth.go rect {
		stroke: var(--ob-accent, var(--ca-accent));
		animation: ob-drop 700ms ease-out both;
	}

	.svc,
	.usr {
		fill: none;
		stroke: var(--ca-text-secondary);
		stroke-width: 2.5;
		stroke-linecap: round;
		stroke-linejoin: round;
	}

	.svc.lit,
	.usr.lit {
		opacity: 1;
		stroke: var(--ob-ok, var(--ca-semantic-success));
	}

	.svc.go,
	.usr.go {
		opacity: 1;
		stroke: var(--ob-accent, var(--ca-accent));
		animation: ob-pop 600ms ease-out both;
	}

	@keyframes ob-drop {
		from {
			transform: translateY(-10px);
			opacity: 0;
		}
		to {
			transform: none;
			opacity: 1;
		}
	}

	@keyframes ob-pop {
		from {
			transform: translateY(-10px) scale(0.6);
			opacity: 0;
		}
	}

	.still *,
	.still {
		animation: none !important;
		transition: none !important;
	}
</style>
