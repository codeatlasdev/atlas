<script lang="ts">
	/** Slide "agents in parallel": two terminals, each an agent in its own worktree, typing and finishing. */
	import { onMount } from "svelte";
	import Frame from "./Frame.svelte";
	import AgentLogo from "#lib/components/AgentLogo.svelte";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { Beats } from "./beats.svelte.ts";

	// The user's own agents when Atlas found them installed (the deck speaks about what they will run), else the two
	// best known. The tasks stay fixed: they are the story, the agent is the actor.
	const FALLBACK = [
		{ id: "claude", label: "Claude Code" },
		{ id: "codex", label: "Codex" },
	];
	const TASKS = [
		{ branch: "feat/checkout", lines: ["› add a coupon field", "✎ src/checkout.ts", "✓ tests pass"] },
		{ branch: "fix/cart-total", lines: ["› fix the rounding bug", "✎ src/cart.ts", "✓ ready to review"] },
	];
	const mine = $derived(agents.list.filter((x) => x.installed));
	// Installed agents first, then the fallbacks they don't already cover: two distinct actors.
	const cast = $derived(
		[...mine, ...FALLBACK.filter((f) => !mine.some((m) => m.id === f.id))].slice(0, TASKS.length),
	);
	const PANES = $derived(
		TASKS.map((t, i) => {
			const who = cast[i]!;
			return {
				...t,
				id: who.id,
				label: who.label,
				lines: [`$ ${who.label.toLowerCase().split(" ")[0]}`, ...t.lines],
			};
		}),
	);
	// One line per beat, the two panes offset by one beat; then a held "done" frame.
	const beats = new Beats([600, 650, 650, 650, 650, 2600]);
	onMount(() => beats.start());
</script>

<Frame title="shop · 2 agents">
	<div class="panes">
		{#each PANES as p, i (p.branch)}
			<div class="pane" class:done={beats.n >= i + 4}>
				<div class="head">
					<AgentLogo agent={p.id} size={12} />
					<b>{p.label}</b>
					<span class="br">{p.branch}</span>
				</div>
				<div class="term" aria-hidden="true">
					{#each p.lines as l, k (k)}
						<div class="ln" class:on={beats.n > k + i - 1} class:ok={l.startsWith("✓")}>{l}</div>
					{/each}
					<span class="caret" class:off={beats.n >= i + 4}></span>
				</div>
			</div>
		{/each}
	</div>
</Frame>

<style>
	/* The terminals are always dark, like a real one: their own small palette, as tokens. */
	.panes {
		--term-bg: #0e1016;
		--ca-text-primary: #d6dbe6;
		--ca-text-secondary: #9aa3b5;
		--ca-semantic-success: #5fd38d;
		--ca-brand-main: #7f95ff;
		position: absolute;
		inset: 0;
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 10px;
		padding: 12px;
	}
	.pane {
		display: flex;
		flex-direction: column;
		border-radius: 10px;
		overflow: hidden;
		background: var(--term-bg);
		box-shadow: 0 0 0 0.5px var(--ca-outline-default);
		transition: box-shadow 400ms ease;
	}
	.pane.done {
		box-shadow: 0 0 0 1px color-mix(in oklab, var(--ca-semantic-success) 70%, transparent);
	}
	.head {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 6px 10px;
		font-size: 11px;
		color: var(--ca-text-secondary);
		border-bottom: 0.5px solid #ffffff14;
	}
	.head b {
		color: var(--ca-text-primary);
		font-weight: 600;
	}
	.br {
		margin-left: auto;
		color: var(--ca-brand-main);
	}
	.term {
		padding: 10px;
		font: 11.5px/1.7 var(--mono);
		color: var(--ca-text-primary);
	}
	.ln {
		opacity: 0;
		clip-path: inset(0 100% 0 0);
		transition:
			opacity 120ms linear,
			clip-path 420ms steps(18, end);
	}
	.ln.on {
		opacity: 1;
		clip-path: inset(0 0 0 0);
	}
	.ln.ok {
		color: var(--ca-semantic-success);
	}
	.caret {
		display: inline-block;
		width: 7px;
		height: 13px;
		background: var(--ca-text-primary);
		animation: blink 1s steps(1) infinite;
	}
	.caret.off {
		visibility: hidden;
	}
	@keyframes blink {
		50% {
			opacity: 0;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.caret {
			animation: none;
		}
	}
</style>
