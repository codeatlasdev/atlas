<script lang="ts">
	/**
	 * One error surface for the whole app. `pane` fills the view that crashed and leaves the rest
	 * of the window alone; `fullscreen` replaces a route; `dialog` is the same body without the
	 * outer frame (inside a Dialog). Actions are opt-in: pass only the ones that make sense.
	 */
	import { Button } from "@codeatlasdev/ui";
	import {
		Bug,
		Check,
		ChevronRight,
		Copy,
		FileText,
		Lightbulb,
		MessageCircleQuestion,
		OctagonAlert,
		RotateCcw,
		Sparkles,
		Wrench,
		X,
	} from "@lucide/svelte";
	import { onMount } from "svelte";
	import { api } from "#lib/api.ts";
	import { launchAgent } from "./agent.ts";
	import {
		aiDiagnosePrompt,
		aiFixPrompt,
		aiIssuePrompt,
		causeHint,
		detailsText,
		issueUrl,
		markdownReport,
		type ErrorReport,
	} from "./report.ts";

	let {
		report,
		variant = "pane",
		title,
		hint,
		retry,
		retryLabel = "Retry",
		close,
		closeLabel = "Close tab",
	}: {
		report: ErrorReport;
		variant?: "pane" | "fullscreen" | "dialog";
		title?: string;
		/** One calm sentence on what still works / what to do. */
		hint?: string;
		retry?: () => void;
		retryLabel?: string;
		close?: () => void;
		closeLabel?: string;
	} = $props();

	let open = $state(false);
	/** Which copy button was last pressed (for its "Copied" tick). */
	let copied = $state<string | null>(null);
	let actions = $state<HTMLElement | null>(null);
	let aiOpen = $state(false);
	const hintCause = $derived(causeHint(report));

	/** Each AI action: copy a ready prompt, or hand it straight to an agent when one can be launched. */
	const ai = $derived([
		{
			id: "fix",
			icon: Wrench,
			label: "Fix it",
			hint: "An agent reproduces it with a failing test, then fixes the cause",
			prompt: () => aiFixPrompt(report),
		},
		{
			id: "issue",
			icon: Bug,
			label: "File an issue",
			hint: "An agent searches duplicates and opens it with gh",
			prompt: () => aiIssuePrompt(report),
		},
		{
			id: "explain",
			icon: MessageCircleQuestion,
			label: "Explain it to me",
			hint: "Plain language, what to do now; works in any chat AI",
			prompt: () => aiDiagnosePrompt(report),
		},
	]);

	// Fullscreen and the dialog have nothing else to focus: land on the primary action (rings only show in
	// keyboard mode, app.css). A crashed pane must not steal focus from wherever the person is working.
	onMount(() => {
		if (variant !== "pane") actions?.querySelector("button")?.focus({ preventScroll: true });
	});

	async function copyText(id: string, text: string) {
		await navigator.clipboard.writeText(text);
		copied = id;
		window.setTimeout(() => copied === id && (copied = null), 1600);
	}

	/** Open an agent terminal with the prompt when there is a worktree to run it in; always copy as well. */
	async function runAi(a: (typeof ai)[number]) {
		const text = a.prompt();
		await copyText(a.id, text);
		aiOpen = false;
		await launchAgent(text);
	}

	const meta = $derived(
		[
			["Where", report.kind ? `${report.where} · ${report.kind}` : report.where],
			["File", report.file],
			["Atlas", report.version],
			["System", [report.context?.os, report.context?.arch].filter(Boolean).join(" ")],
			["Terminal host", report.context?.hostVersion],
		].filter((r): r is [string, string] => !!r[1]),
	);
	const heading = $derived(title ?? "Something went wrong");
</script>

<div class="wrap {variant}" role="alert" aria-labelledby="ep-title">
	<div class="card">
		<div class="top">
			<span class="icon" aria-hidden="true"
				><OctagonAlert size={variant === "fullscreen" ? 22 : 18} strokeWidth={1.75} /></span
			>
			<div class="head">
				<h2 id="ep-title">{heading}</h2>
				{#if hint}<p class="hint">{hint}</p>{/if}
			</div>
		</div>

		<pre class="msg ca-selectable">{report.message}</pre>

		<div class="actions" bind:this={actions}>
			{#if retry}
				<Button size="sm" variant="filled" onclick={retry}>
					{#snippet leading()}<RotateCcw size={13} />{/snippet}
					{retryLabel}
				</Button>
			{/if}
			{#if close}
				<Button size="sm" variant={retry ? "glass" : "filled"} onclick={close}>
					{#snippet leading()}<X size={13} />{/snippet}
					{closeLabel}
				</Button>
			{/if}
			<Button size="sm" variant="ghost" onclick={() => (aiOpen = !aiOpen)} aria-expanded={aiOpen}>
				{#snippet leading()}<Sparkles size={13} />{/snippet}
				Ask AI
			</Button>
			<Button size="sm" variant="ghost" onclick={() => copyText("details", detailsText(report))}>
				{#snippet leading()}{#if copied === "details"}<Check size={13} />{:else}<Copy
							size={13}
						/>{/if}{/snippet}
				{copied === "details" ? "Copied" : "Copy details"}
			</Button>
			<Button size="sm" variant="ghost" onclick={() => api.openUrl(issueUrl(report))}>
				{#snippet leading()}<Bug size={13} />{/snippet}
				Report issue
			</Button>
		</div>

		{#if hintCause}
			<div class="cause" role="note">
				<span class="bulb" aria-hidden="true"><Lightbulb size={14} /></span>
				<div>
					<b>{hintCause.title}</b>
					<p>{hintCause.detail}</p>
				</div>
			</div>
		{/if}

		{#if aiOpen}
			<div class="ai" role="group" aria-label="Ask AI">
				<p class="ai-note">
					Each one copies a complete prompt (error, environment, what the app did, stack) and opens an agent
					with it when a workspace is open. Nothing leaves your machine until you send it.
				</p>
				{#each ai as a (a.id)}
					<button class="ai-row" onclick={() => runAi(a)}>
						<span class="ai-ico" aria-hidden="true"><a.icon size={14} /></span>
						<span class="ai-text">
							<b>{a.label}</b>
							<span>{a.hint}</span>
						</span>
						<span class="ai-tick"
							>{#if copied === a.id}<Check size={13} />{:else}<Copy size={12} />{/if}</span
						>
					</button>
				{/each}
				<button class="ai-row" onclick={() => copyText("md", markdownReport(report))}>
					<span class="ai-ico" aria-hidden="true"><FileText size={14} /></span>
					<span class="ai-text">
						<b>Copy as Markdown</b>
						<span>The full report, to paste anywhere</span>
					</span>
					<span class="ai-tick"
						>{#if copied === "md"}<Check size={13} />{:else}<Copy size={12} />{/if}</span
					>
				</button>
			</div>
		{/if}

		<div class="more">
			<button class="toggle" aria-expanded={open} aria-controls="ep-details" onclick={() => (open = !open)}>
				<span class="chev" class:open><ChevronRight size={12} /></span>
				Details
			</button>
			{#if open}
				<div id="ep-details" class="details ca-selectable">
					<dl>
						{#each meta as [k, v] (k)}
							<dt>{k}</dt>
							<dd>{v}</dd>
						{/each}
					</dl>
					{#if report.context?.breadcrumbs.length}
						<div class="crumbs">
							<span class="cap">Last things the app did</span>
							<ol>
								{#each report.context.breadcrumbs.slice(-8) as b (b.at + b.text)}
									<li>{b.text}</li>
								{/each}
							</ol>
						</div>
					{/if}
					{#if report.stack}<pre class="stack">{report.stack}</pre>{/if}
				</div>
			{/if}
		</div>
	</div>
</div>

<style>
	.wrap {
		flex: 1;
		min-width: 0;
		min-height: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 24px;
		overflow: auto;
		color: var(--ca-text-primary);
	}
	.wrap.fullscreen {
		position: fixed;
		inset: 0;
		z-index: 2000;
		padding: 40px;
		background: var(--ca-surface-base);
	}
	.wrap.dialog {
		padding: 0;
		overflow: visible;
		align-items: stretch;
	}
	.card {
		width: 100%;
		max-width: 520px;
		display: flex;
		flex-direction: column;
		gap: 14px;
	}
	.fullscreen .card {
		max-width: 560px;
		gap: 18px;
	}
	.pane .card {
		padding: 20px;
		border-radius: 12px;
		background: var(--ca-surface-raised);
		box-shadow:
			0 0 0 0.5px color-mix(in oklab, var(--ca-semantic-error) 32%, var(--ca-outline-subtle)),
			var(--ca-shadow-xs, 0 1px 2px rgb(0 0 0 / 0.08));
		animation: rise var(--dur) var(--ease) both;
	}
	.top {
		display: flex;
		align-items: flex-start;
		gap: 12px;
	}
	.icon {
		flex: none;
		display: grid;
		place-items: center;
		width: 32px;
		height: 32px;
		border-radius: 9px;
		color: var(--ca-semantic-error);
		background: color-mix(in oklab, var(--ca-semantic-error) 12%, transparent);
	}
	.fullscreen .icon {
		width: 40px;
		height: 40px;
		border-radius: 11px;
	}
	.head {
		min-width: 0;
		padding-top: 1px;
	}
	h2 {
		margin: 0;
		font-size: 14px;
		font-weight: 600;
		line-height: 1.3;
		letter-spacing: -0.005em;
	}
	.fullscreen h2 {
		font-size: 18px;
		letter-spacing: -0.012em;
	}
	.hint {
		margin: 3px 0 0;
		font-size: 12.5px;
		line-height: 1.45;
		color: var(--ca-text-secondary);
	}
	.fullscreen .hint {
		font-size: 13.5px;
	}
	.msg {
		margin: 0;
		padding: 9px 11px;
		max-height: 112px;
		overflow: auto;
		border-radius: 8px;
		font: 11.5px/1.5 var(--mono);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		color: var(--ca-text-primary);
		background: color-mix(in oklab, var(--ca-text-primary) 5%, transparent);
		user-select: text;
		-webkit-user-select: text;
		cursor: text;
	}
	.actions {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 6px;
	}
	.more {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.toggle {
		align-self: flex-start;
		display: inline-flex;
		align-items: center;
		gap: 4px;
		margin-left: -2px;
		padding: 2px 4px 2px 2px;
		border: 0;
		border-radius: 5px;
		background: none;
		font: inherit;
		font-size: 12px;
		color: var(--ca-text-secondary);
		cursor: default;
	}
	.toggle:hover {
		color: var(--ca-text-primary);
		background: var(--ca-action-hover);
	}
	.chev {
		display: inline-flex;
		transition: transform var(--dur-fast) var(--ease);
	}
	.chev.open {
		transform: rotate(90deg);
	}
	.details {
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: 10px 12px;
		border-radius: 8px;
		box-shadow: inset 0 0 0 0.5px var(--ca-outline-subtle);
		user-select: text;
		-webkit-user-select: text;
		cursor: text;
	}
	dl {
		display: grid;
		grid-template-columns: max-content 1fr;
		gap: 3px 14px;
		margin: 0;
		font-size: 12px;
	}
	dt {
		color: var(--ca-text-secondary);
	}
	dd {
		margin: 0;
		min-width: 0;
		overflow-wrap: anywhere;
		font-family: var(--mono);
		font-size: 11.5px;
	}
	.stack {
		margin: 0;
		padding-top: 10px;
		border-top: 0.5px solid var(--ca-outline-subtle);
		max-height: 168px;
		overflow: auto;
		font: 11px/1.55 var(--mono);
		color: var(--ca-text-secondary);
		white-space: pre;
	}
	.cause {
		display: flex;
		gap: 9px;
		padding: 10px 12px;
		border-radius: 9px;
		font-size: 12.5px;
		line-height: 1.45;
		background: color-mix(in oklab, var(--ca-semantic-warning, #e0a030) 10%, transparent);
		box-shadow: inset 0 0 0 0.5px color-mix(in oklab, var(--ca-semantic-warning, #e0a030) 30%, transparent);
	}
	.cause p {
		margin: 2px 0 0;
		color: var(--ca-text-secondary);
	}
	.bulb {
		flex: none;
		padding-top: 1px;
		color: var(--ca-semantic-warning, #e0a030);
	}
	.ai {
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding: 6px;
		border-radius: 10px;
		box-shadow: inset 0 0 0 0.5px var(--ca-outline-subtle);
		animation: rise var(--dur) var(--ease) both;
	}
	.ai-note {
		margin: 2px 6px 6px;
		font-size: 11.5px;
		line-height: 1.45;
		color: var(--ca-text-secondary);
	}
	.ai-row {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 7px 8px;
		border: 0;
		border-radius: 7px;
		background: none;
		font: inherit;
		text-align: left;
		color: var(--ca-text-primary);
		cursor: default;
	}
	.ai-row:hover {
		background: var(--ca-action-hover);
	}
	.ai-ico {
		flex: none;
		display: grid;
		place-items: center;
		width: 24px;
		height: 24px;
		border-radius: 6px;
		color: var(--ca-accent, currentColor);
		background: color-mix(in oklab, var(--ca-text-primary) 6%, transparent);
	}
	.ai-text {
		display: flex;
		flex-direction: column;
		min-width: 0;
		flex: 1;
	}
	.ai-text b {
		font-size: 12.5px;
		font-weight: 550;
	}
	.ai-text span {
		font-size: 11.5px;
		color: var(--ca-text-secondary);
	}
	.ai-tick {
		flex: none;
		color: var(--ca-text-tertiary, var(--ca-text-secondary));
	}
	.crumbs .cap {
		font-size: 11px;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--ca-text-secondary);
	}
	.crumbs ol {
		margin: 4px 0 0;
		padding-left: 16px;
		font: 11px/1.55 var(--mono);
		color: var(--ca-text-secondary);
	}
	@keyframes rise {
		from {
			opacity: 0;
			transform: translateY(4px);
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.pane .card {
			animation: none;
		}
		.chev {
			transition: none;
		}
	}
</style>
