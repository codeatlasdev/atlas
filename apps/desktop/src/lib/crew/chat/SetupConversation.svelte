<script lang="ts">
	/**
	 * SetupConversation — the first-run chat of a new crewmate: it asks name, role, project and approval one
	 * message at a time (lettered options, name chips, or a free-text composer) and confirms with a summary.
	 * The state machine lives in setup.svelte.ts; this renders it.
	 */
	import type { Crewmate } from "#lib/crew.ts";
	import CrewTyping from "#lib/crew/CrewTyping.svelte";
	import type { CrewmateLook } from "#lib/crew/look.ts";
	import { msgIn, msgOut, optionIn } from "#lib/crew/motion.ts";
	import type { SetupStep } from "#lib/crew/crewSetup.ts";
	import Composer from "./Composer.svelte";
	import { SetupFlow, type IdeProject } from "./setup.svelte.ts";
	import "./chat.css";

	interface Props {
		crewmate: Crewmate;
		step: SetupStep;
		ideProjects: IdeProject[];
	}

	let { crewmate, step, ideProjects }: Props = $props();

	const setup = new SetupFlow();
	const ideProjectNames = $derived(ideProjects.map((p) => p.name));
	const answer = (key?: string, text?: string) => setup.answer(step, crewmate, ideProjects, key, text);

	/** Re-run when a crewmate with a setup step is selected, or its step changes. */
	$effect(() => setup.resume(step, crewmate, ideProjectNames));

	const placeholder = $derived(
		step === "name"
			? "Type a name…"
			: step === "project"
				? "Paste a git URL or server path, or skip"
				: "Type your answer…",
	);
	const look = $derived(crewmate.look as CrewmateLook | undefined);
</script>

{#snippet bubbleText(parts: { text: string; bold?: true }[] | undefined, text: string)}
	{#if parts}{#each parts as p, pi (pi)}{#if p.bold}<strong>{p.text}</strong
				>{:else}{p.text}{/if}{/each}{:else}{text}{/if}
{/snippet}

<div class="thread setup-thread" aria-label="Setup conversation" aria-live="polite">
	{#each setup.messages as msg, i (i)}
		{#if msg.kind === "crewmate"}
			<div class="msg-in" in:msgIn>
				<div class="msg-in-body">
					<div class="bubble in">{@render bubbleText(msg.parts, msg.text)}</div>
				</div>
			</div>
		{:else}
			<div class="msg-out" in:msgOut>
				<div class="bubble out">{msg.text}</div>
			</div>
		{/if}
	{/each}

	{#if setup.typing}
		<div class="msg-in setup-typing" in:msgIn>
			<CrewTyping seed={crewmate.id} {look} label="Thinking" />
		</div>
	{/if}

	{#if setup.question}
		{@const q = setup.question}
		<div class="msg-in" in:msgIn>
			<div class="msg-in-body">
				<div class="bubble in">{@render bubbleText(q.parts, q.text)}</div>
				{#if q.options}
					<div class="ask-card setup-ask">
						{#each q.options as opt, oi (opt.key)}
							<button
								class="ask-option"
								in:optionIn={{ index: oi }}
								onclick={() => void answer(opt.key)}
								disabled={setup.saving}
							>
								<span class="ask-key">{opt.key}</span>
								<div class="ask-opt-body">
									<span class="ask-opt-title">{opt.title}</span>
									{#if opt.subtitle}<span class="ask-opt-sub">{opt.subtitle}</span>{/if}
								</div>
							</button>
						{/each}
					</div>
				{/if}
				{#if q.nameChips?.length}
					<div class="name-chips">
						{#each q.nameChips as chip (chip)}
							<button class="name-chip" onclick={() => void answer(undefined, chip)} disabled={setup.saving}
								>{chip}</button
							>
						{/each}
					</div>
				{/if}
			</div>
		</div>
	{/if}
</div>

{#if setup.question?.allowText}
	<Composer
		bind:value={setup.input}
		{placeholder}
		label="Setup answer"
		busy={setup.saving}
		onsend={() => void answer(undefined, setup.input)}
	/>
{/if}

<style>
	.thread.setup-thread {
		padding-top: 24px;
	}
	.setup-typing {
		padding: 4px 16px;
	}
	.setup-ask {
		margin-top: 8px;
	}
	.name-chips {
		display: flex;
		gap: 8px;
		flex-wrap: wrap;
		margin-top: 10px;
		padding: 0 2px;
	}
	.name-chip {
		padding: 6px 16px;
		border-radius: 999px;
		border: 1px solid var(--ca-outline-subtle);
		background: var(--ca-surface-sunken);
		color: var(--ca-text-primary);
		font-size: 14px;
		cursor: default;
		transition: background 80ms;
	}
	.name-chip:hover:not(:disabled) {
		background: color-mix(in oklab, var(--ca-brand-main) 12%, var(--ca-surface-sunken));
		border-color: color-mix(in oklab, var(--ca-brand-main) 40%, transparent);
	}
	.name-chip:disabled {
		opacity: 0.5;
	}
</style>
