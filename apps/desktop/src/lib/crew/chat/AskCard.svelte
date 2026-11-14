<script lang="ts">
	/**
	 * AskCard — the card a crewmate answers a task with when it needs a choice: the question with a dismiss
	 * button, lettered options, and an optional "type your own answer" box. Picking or typing calls `onanswer`.
	 */
	import type { AskPayload } from "./parseAsk.ts";

	interface Props {
		ask: AskPayload;
		onanswer: (text: string) => void;
		ondismiss: () => void;
	}

	let { ask, onanswer, ondismiss }: Props = $props();
</script>

<div class="ask-card">
	<div class="ask-question">
		<span>{ask.question}</span>
		<button class="ask-dismiss" aria-label="Dismiss" onclick={ondismiss}>×</button>
	</div>
	{#each ask.options as opt, oi (opt.key)}
		<button class="ask-option" onclick={() => onanswer(opt.title)}>
			<span class="ask-key">{String.fromCharCode(65 + oi)}</span>
			<div class="ask-opt-body">
				<span class="ask-opt-title">{opt.title}</span>
				{#if opt.subtitle}<span class="ask-opt-sub">{opt.subtitle}</span>{/if}
			</div>
		</button>
	{/each}
	{#if ask.allow_text !== false}
		<input
			class="ask-text"
			placeholder="Type your own answer"
			onkeydown={(e) => {
				const v = (e.currentTarget as HTMLInputElement).value.trim();
				if (e.key === "Enter" && v) onanswer(v);
			}}
		/>
	{/if}
</div>

<style>
	.ask-question {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 14px 18px 12px;
		font-size: 15px;
		font-weight: 500;
		color: var(--ca-text-primary);
		border-bottom: 0.5px solid var(--ca-outline-subtle);
	}
	.ask-dismiss {
		background: none;
		border: none;
		color: var(--ca-text-secondary);
		cursor: default;
		font-size: 16px;
		line-height: 1;
		padding: 0 2px;
		flex: none;
	}
	.ask-dismiss:hover {
		color: var(--ca-text-primary);
	}
	.ask-text {
		display: block;
		width: 100%;
		padding: 12px 18px;
		background: none;
		border: none;
		border-top: 0.5px solid var(--ca-outline-subtle);
		font-size: 14px;
		color: var(--ca-text-secondary);
		outline: none;
		box-sizing: border-box;
	}
	.ask-text::placeholder {
		color: var(--ca-text-secondary);
		opacity: 0.6;
	}
	.ask-text:focus {
		color: var(--ca-text-primary);
	}
</style>
