<!--
  The art of the connect step: this computer -> SSH link -> the server, lit phase by phase while the install runs.
  Idle: the link breathes. Each phase lights its own part (a key crosses the link, the server pulses on the check, a
  package drops in, a user badge pops, the service powers on, the key lands in authorized_keys, the link goes live).
  A failure turns the part that broke red and the link on the server's side with it. Only transform and opacity move;
  `reducedMotion` shows the states still. Colours are the app tokens (named in onboarding.css, `--ob-*`).
-->
<script lang="ts">
	import { reducedMotion } from "#lib/crew/motion.ts";
	import { IS_MAC, IS_WIN } from "#lib/keys.ts";
	import type { InstallPhase } from "#lib/crew/installSteps.ts";

	let {
		done,
		active,
		failed,
	}: { done: Set<InstallPhase>; active: InstallPhase | null; failed: InstallPhase | null } = $props();

	/** The machine the person sits at, named the way their OS does. */
	const machine = IS_MAC ? "your Mac" : IS_WIN ? "this PC" : "this computer";
	const lit = (p: InstallPhase) => done.has(p) || active === p;
	const live = $derived(done.has("done"));
	/** The link is red on the side where the failing phase happens: signing in breaks it near the server's door. */
	const broken = $derived(failed !== null);
</script>

<div
	class="scene"
	class:still={reducedMotion}
	data-testid="connect-scene"
	data-active={active ?? ""}
	data-failed={failed ?? ""}
>
	<svg viewBox="0 0 520 260" role="img" aria-label="{machine} connecting to your server">
		<!-- the link -->
		<line class="link" x1="150" y1="130" x2="370" y2="130" />
		<line class="link flow" class:on={active !== null && !broken} x1="150" y1="130" x2="370" y2="130" />
		<line class="link bad" class:on={broken} x1="260" y1="130" x2="370" y2="130" />
		<line class="link good" class:on={live} x1="150" y1="130" x2="370" y2="130" />

		<!-- this computer -->
		<g class="node me" class:lit={lit("signin")}>
			<rect x="60" y="92" width="90" height="62" rx="9" />
			<rect x="48" y="156" width="114" height="9" rx="4.5" class="base" />
			<text x="105" y="190" text-anchor="middle">{machine}</text>
		</g>

		<!-- the key that crosses the link while signing in -->
		<g class="key" class:go={active === "signin"} class:placed={lit("ssh")}>
			<circle cx="0" cy="0" r="9" />
			<rect x="8" y="-2.5" width="22" height="5" rx="2.5" />
			<rect x="22" y="2" width="4" height="7" rx="1.5" />
		</g>

		<!-- the server -->
		<g class="node srv" class:lit={lit("check")} class:pulse={active === "check"} class:bad={failed !== null}>
			<rect x="370" y="68" width="96" height="30" rx="7" />
			<rect x="370" y="102" width="96" height="30" rx="7" />
			<rect x="370" y="136" width="96" height="30" rx="7" />
			<circle cx="384" cy="83" r="3.5" class="led" />
			<circle cx="384" cy="117" r="3.5" class="led" />
			<circle cx="384" cy="151" r="3.5" class="led" />
			<text x="418" y="190" text-anchor="middle">your server</text>
		</g>

		<!-- phase glyphs over the server -->
		<g class="drop pkg" class:go={active === "install"} class:lit={lit("install")}>
			<rect x="395" y="22" width="22" height="22" rx="4" />
			<path d="M395 30h22M406 22v22" />
		</g>
		<g class="drop badge" class:go={active === "user"} class:lit={lit("user")}>
			<circle cx="438" cy="34" r="11" />
			<circle cx="438" cy="31" r="3.5" class="hd" />
			<path d="M431 41c1-5 13-5 14 0" />
		</g>
		<g class="drop svc" class:go={active === "service"} class:lit={lit("service")}>
			<path d="M497 40l-9 13h7l-3 11 11-14h-7z" />
		</g>
		<g class="drop auth" class:go={active === "ssh"} class:lit={lit("ssh")}>
			<rect x="372" y="206" width="94" height="22" rx="6" />
			<text x="419" y="221" text-anchor="middle">authorized_keys</text>
		</g>
	</svg>
</div>

<style>
	.scene {
		width: min(480px, 92%);
		color: var(--ob-line, var(--ca-text-secondary));
	}

	svg {
		width: 100%;
		height: auto;
		overflow: visible;
	}

	.link {
		stroke: var(--ob-line-idle, var(--ca-border-subtle));
		stroke-width: 3;
		stroke-linecap: round;
		fill: none;
	}

	.link.flow,
	.link.bad,
	.link.good {
		opacity: 0;
		transition: opacity 300ms;
	}

	.link.flow {
		stroke: var(--ob-accent, var(--ca-accent));
		stroke-dasharray: 8 14;
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

	@keyframes ob-dash {
		to {
			stroke-dashoffset: -22;
		}
	}

	.node rect {
		fill: var(--ca-surface-sunken);
		stroke: var(--ob-line-idle, var(--ca-border-subtle));
		stroke-width: 2;
		transition: stroke 300ms;
	}

	.node .base {
		fill: var(--ob-line-idle, var(--ca-border-subtle));
		stroke: none;
	}

	.node.lit rect {
		stroke: var(--ob-accent, var(--ca-accent));
	}

	.node.bad rect {
		stroke: var(--ob-danger, var(--ca-danger));
	}

	.node text,
	.auth text {
		fill: var(--ca-text-secondary);
		font-size: 13px;
	}

	.led {
		fill: var(--ob-line-idle, var(--ca-border-subtle));
		transition: fill 300ms;
	}

	.srv.lit .led {
		fill: var(--ob-ok, var(--ca-semantic-success));
	}

	.srv.pulse {
		animation: ob-pulse 900ms ease-in-out infinite;
		transform-origin: 418px 117px;
		transform-box: view-box;
	}

	@keyframes ob-pulse {
		50% {
			transform: scale(1.05);
		}
	}

	/* the key: rests at the computer, crosses the link while signing in, then waits at the server's door */
	.key {
		transform: translate(160px, 130px);
		opacity: 0.35;
		fill: var(--ob-key, var(--ca-accent));
		transition: opacity 300ms;
	}

	.key.go {
		opacity: 1;
		animation: ob-cross 1400ms ease-in-out infinite;
	}

	.key.placed {
		opacity: 1;
		transform: translate(330px, 130px);
	}

	@keyframes ob-cross {
		from {
			transform: translate(160px, 130px);
		}
		to {
			transform: translate(330px, 130px);
		}
	}

	/* what each phase puts on the server: dim until its phase, then in colour; the active one moves once */
	.drop {
		opacity: 0.22;
		transition: opacity 300ms;
		fill: none;
		stroke: var(--ca-text-secondary);
		stroke-width: 2;
		stroke-linecap: round;
		stroke-linejoin: round;
	}

	.drop.lit {
		opacity: 1;
		stroke: var(--ob-ok, var(--ca-semantic-success));
	}

	.drop.go {
		stroke: var(--ob-accent, var(--ca-accent));
		animation: ob-drop 700ms ease-out both;
	}

	.drop .hd {
		fill: none;
	}

	.auth rect {
		fill: var(--ca-surface-sunken);
	}

	.auth text {
		stroke: none;
	}

	@keyframes ob-drop {
		from {
			transform: translateY(-14px);
			opacity: 0;
		}
		to {
			transform: none;
			opacity: 1;
		}
	}

	.still * {
		animation: none !important;
		transition: none !important;
	}
</style>
