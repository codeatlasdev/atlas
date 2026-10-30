<script lang="ts">
	/**
	 * CrewViewBar — top-edge hover switcher for the Crew workspace.
	 * Pattern mirrors WorkspaceRail: a thin hot-zone at the top edge reveals
	 * a segmented Chat / Board control after SHOW_DELAY_MS.
	 *
	 * Positioned to the right of col-left (270 px) so it never covers the
	 * macOS traffic-light area or the left column's drag region.
	 */
	import { MessageSquare, Columns3 } from "@lucide/svelte";
	import { SHOW_DELAY_MS, HIDE_DELAY_MS } from "#lib/shell/workspaceRail.ts";
	import { crewView } from "#lib/bots/crewView.svelte.ts";
	import { shortcutFor } from "#lib/keymap.svelte.ts";
	import { reducedMotion } from "#lib/bots/motion.ts";

	let visible = $state(false);
	let hideTimer: ReturnType<typeof setTimeout> | undefined;
	let showTimer: ReturnType<typeof setTimeout> | undefined;

	const shortcut = $derived(shortcutFor("atlas::ToggleCrewBoard") ?? "Cmd+Shift+K");

	const HINT_KEY = "atlas.crewViewBarHintSeen";
	let hint = $state(
		typeof localStorage !== "undefined" && localStorage.getItem(HINT_KEY) !== "1",
	);

	/**
	 * Watch the pointer instead of a hover strip. The strip was 6 px at the very top, which on macOS is the window's
	 * resize edge: the system takes the mouse there and the page never saw `mouseenter`, so the bar never opened. A
	 * listener on the window also blocks no clicks (the chat header sits right below).
	 */
	const EDGE_PX = 16;
	/** Left of this is the traffic lights and the bot list's drag area. */
	const LEFT_CLEAR_PX = 80;
	let near = $state(false);
	function onPointerMove(e: PointerEvent) {
		const atEdge = e.clientY <= EDGE_PX && e.clientX >= LEFT_CLEAR_PX;
		if (atEdge === near) return;
		near = atEdge;
		if (atEdge) onHotZoneEnter();
		// Leaving the edge starts the hide, unless the pointer went onto the bar (its mouseenter already ran: the
		// browser fires it before this window pointermove, so it cannot cancel a hide started here).
		else if (!overBar) onBarLeave();
	}

	function onHotZoneEnter() {
		clearTimeout(hideTimer);
		showTimer = setTimeout(() => {
			visible = true;
		}, SHOW_DELAY_MS);
	}

	let overBar = false;
	function onBarLeave() {
		overBar = false;
		clearTimeout(showTimer);
		hideTimer = setTimeout(() => {
			visible = false;
		}, HIDE_DELAY_MS);
	}

	function onBarEnter() {
		overBar = true;
		clearTimeout(hideTimer);
	}

	function pick(v: "chat" | "board") {
		crewView.value = v;
		visible = false;
	}

	$effect(() => {
		if (visible && hint) {
			hint = false;
			try {
				localStorage.setItem(HINT_KEY, "1");
			} catch {
				/* storage full */
			}
		}
	});
</script>

<svelte:window onpointermove={onPointerMove} />

<!-- Always-visible horizontal handle: anchors the discoverable affordance -->
<div class="edge-handle" class:near class:hidden={visible} aria-hidden="true"></div>

{#if hint && !visible}
	<button class="edge-hint" onmouseenter={onHotZoneEnter} onclick={() => (visible = true)}>
		Board <span class="kbd">{shortcut}</span>
	</button>
{/if}

<!-- Segmented control: slides down from the top on hover -->
<div
	class="bar"
	class:visible
	class:reduced={reducedMotion}
	role="toolbar"
	tabindex="-1"
	aria-label="Crew view switcher"
	onmouseenter={onBarEnter}
	onmouseleave={onBarLeave}
>
	<button
		class="seg"
		class:active={crewView.value === "chat"}
		onclick={() => pick("chat")}
		aria-label="Chat view"
		aria-pressed={crewView.value === "chat"}
	>
		<MessageSquare size={13} />
		Chat
	</button>
	<button
		class="seg"
		class:active={crewView.value === "board"}
		onclick={() => pick("board")}
		aria-label="Board view"
		aria-pressed={crewView.value === "board"}
	>
		<Columns3 size={13} />
		Board
	</button>
</div>

<style>
	/* Always-visible horizontal pill at the top center — makes the switcher findable. Below the resize edge. */
	/* Centred on the content beside the bot list (270px) in Chat, on the window in Board (the list is hidden there):
	   centred on the window, it sat off the chat's centre, worst in full screen. */
	:global(:root) {
		--crew-center: calc(50% + 135px);
	}
	:global(:root:has(.col-left[hidden])) {
		--crew-center: 50%;
	}
	.edge-handle {
		position: fixed;
		left: var(--crew-center);
		top: 6px;
		translate: -50% 0;
		width: 28px;
		height: 3px;
		border-radius: 2px;
		background: var(--ca-text-secondary);
		opacity: 0.3;
		z-index: 1702;
		pointer-events: none;
		transition:
			opacity 150ms ease,
			width 150ms ease;
	}

	.edge-handle.near {
		opacity: 0.8;
		width: 40px;
	}

	.edge-handle.hidden {
		opacity: 0;
	}

	/* First-run hint label */
	.edge-hint {
		position: fixed;
		left: var(--crew-center);
		top: 10px;
		translate: -50% 0;
		z-index: 1702;
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 4px 10px;
		border: 0.5px solid var(--ca-outline-subtle);
		border-radius: 999px;
		background: var(--ca-surface-base);
		color: var(--ca-text-secondary);
		font-size: 11.5px;
		box-shadow: var(--ca-shadow-sm);
		cursor: default;
		animation: hint-in 400ms ease-out 800ms both;
	}

	.edge-hint .kbd {
		opacity: 0.65;
		font-size: 10.5px;
	}

	@keyframes hint-in {
		from {
			opacity: 0;
			transform: translateY(-5px);
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.edge-hint {
			animation: none;
		}
	}

	/* Segmented control: slides down from hidden above the top edge. */
	.bar {
		position: fixed;
		left: var(--crew-center);
		top: 0;
		translate: -50% -100%;
		z-index: 1701;
		display: flex;
		gap: 2px;
		padding: 5px;
		background: var(--ca-surface-overlay, var(--ca-surface-base));
		border: 0.5px solid var(--ca-outline-subtle);
		border-top: none;
		border-radius: 0 0 10px 10px;
		box-shadow: var(--ca-shadow-sm);
		transition: translate 180ms ease-out;
	}

	.bar.visible {
		translate: -50% 0;
	}

	.bar.reduced {
		transition: none;
	}

	.seg {
		display: flex;
		align-items: center;
		gap: 5px;
		padding: 4px 10px;
		border: none;
		border-radius: 6px;
		background: none;
		color: var(--ca-text-secondary);
		font-size: 12px;
		font-weight: 500;
		cursor: default;
		transition:
			background 120ms ease,
			color 120ms ease;
	}

	.seg:hover {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
	}

	.seg.active {
		background: var(--ca-brand-main);
		color: var(--ca-surface-base);
	}

	.seg.active:hover {
		background: var(--ca-brand-main);
		color: var(--ca-surface-base);
		opacity: 0.9;
	}
</style>
