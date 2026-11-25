<script lang="ts">
	/**
	 * The project steps of the welcome, one per phase of the first-run machine (flow.ts): find projects, pick them,
	 * start the first stack, done. Shared by the first-run deck's last slide and the standing empty state (Hero).
	 */
	import { Button, Checkbox, Kbd, Spinner } from "@codeatlasdev/ui";
	import { ArrowRight, Check, FolderPlus, Sparkles, Wand2 } from "@lucide/svelte";
	import { kbd } from "#lib/keys.ts";
	import { tilde } from "#lib/format.ts";
	import { ago } from "#lib/components/sidebar/discovery.ts";
	import { discovery } from "#lib/stores/discovery.svelte.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { ACTIONS, choiceMeta } from "./flow.ts";
	import { welcome as w } from "./welcome.svelte.ts";
	import { formatNumber } from "#lib/i18n/fmt.ts";
	import { aroundSlot } from "#lib/i18n/slot.ts";
	import { m } from "#lib/i18n/m.ts";
	import { stateLabel } from "#lib/panel/stateLabel.ts";

	let { onfinish }: { onfinish: () => void } = $props();

	let live = $derived(store.live[w.watching ?? ""]?.services ?? []);

	// Watch the stack we started; the machine decides when it is up or has failed.
	$effect(() => {
		void w.started;
		w.observe();
	});

	const seconds = $derived(
		formatNumber(discovery.elapsedMs / 1000, { minimumFractionDigits: 1, maximumFractionDigits: 1 }),
	);
</script>

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

{#snippet welcome()}
	<h1>{m.onboarding_phases_welcome_title_first()}<br />{m.onboarding_phases_welcome_title_second()}</h1>
	<p class="lead">{m.onboarding_phases_welcome_lead()}</p>
	<div class="actions">
		<Button size="lg" data-testid={ACTIONS.scan} onclick={() => void w.scan()}>
			{#snippet leading()}<Sparkles size={15} />{/snippet}
			{m.onboarding_phases_find_my_projects()}
		</Button>
		<div class="secondary">
			<button class="link" data-testid={ACTIONS.addFolder} onclick={() => void w.addFolder()}>
				<FolderPlus size={13} />
				{m.onboarding_phases_add_a_folder()}
			</button>
		</div>
	</div>
{/snippet}

{#snippet scanning()}
	{@const title = aroundSlot((number) =>
		m.onboarding_phases_scanning_title({ count: w.choices.length, number }),
	)}
	<h1 aria-live="polite">
		{title[0]}<span class="count">{formatNumber(w.choices.length)}</span>{title[1]}
	</h1>
	<p class="lead">
		<Spinner size={12} />
		{m.onboarding_phases_scanning_progress({ count: discovery.dirs, seconds })}
	</p>
	<ul class="live" aria-label={m.onboarding_phases_projects_found()}>
		{#each w.choices.slice(0, 5) as f (f.path)}
			<li>
				<span class="dot"></span>
				<b>{f.name}</b>
				<span class="meta">{choiceMeta(f)}</span>
			</li>
		{/each}
	</ul>
	<p class="cur mono" title={discovery.current}>{tilde(discovery.current)}</p>
	<div class="actions">
		<Button variant="ghost" onclick={() => void w.cancelScan()}>{m.onboarding_phases_stop()}</Button>
	</div>
{/snippet}

{#snippet empty()}
	<h1>{m.onboarding_phases_no_projects_found_yet()}</h1>
	<p class="lead">
		{`${m.onboarding_phases_scanned_folders({ count: discovery.done?.dirs ?? 0 })} ${m.onboarding_phases_point_at_folder()}`}
	</p>
	<div class="actions">
		<Button size="lg" data-testid={ACTIONS.init} onclick={() => void w.initFolder()}>
			{#snippet leading()}<Wand2 size={15} />{/snippet}
			{m.onboarding_phases_choose_a_project_folder()}
		</Button>
		<div class="secondary">
			<button class="link" data-testid={ACTIONS.addFolder} onclick={() => void w.addFolder()}>
				<FolderPlus size={13} />
				{m.onboarding_phases_add_a_folder()}
			</button>
			<button class="link" onclick={() => void w.scan()}>{m.onboarding_phases_scan_again()}</button>
		</div>
	</div>
{/snippet}

{#snippet results()}
	{@const found = aroundSlot((number) =>
		m.onboarding_phases_found_title({ count: w.choices.length, number }),
	)}
	<h1>
		{found[0]}<span class="count">{formatNumber(w.choices.length)}</span>{found[1]}
	</h1>
	<p class="lead">
		{#if w.summary.services}{m.onboarding_phases_services_ready({
				count: w.summary.services,
			})}{:else}{m.onboarding_phases_detects_how_to_run()}{/if}
		{#if w.summary.broken}{m.onboarding_phases_configs_need_look({ count: w.summary.broken })}{/if}
	</p>
	<ul class="pick" aria-label={m.onboarding_phases_projects_to_add()}>
		{#each w.choices.slice(0, 7) as f (f.path)}
			<li>
				<Checkbox
					checked={w.picked[f.path] ?? true}
					onchange={() => w.toggle(f.path)}
					aria-label={m.onboarding_phases_add_name({ name: f.name })}
				/>
				<span class="name"><b>{f.name}</b><span class="meta mono">{tilde(f.path)}</span></span>
				<span class="meta">{choiceMeta(f)} · {ago(f.modified)}</span>
			</li>
		{/each}
	</ul>
	{#if w.choices.length > 7}<p class="more">
			{m.onboarding_phases_more_projects({ count: w.choices.length - 7 })}
		</p>{/if}
	<div class="actions">
		<Button
			size="lg"
			data-testid={ACTIONS.add}
			disabled={!w.chosen.length}
			onclick={() => void w.addChosen()}
		>
			{m.onboarding_phases_add_and_start({ count: w.chosen.length })}
			{#snippet trailing()}<ArrowRight size={15} />{/snippet}
		</Button>
		<div class="secondary">
			<button class="link" onclick={() => void w.scan()}>{m.onboarding_phases_scan_again()}</button>
		</div>
	</div>
{/snippet}

{#snippet starting()}
	<h1>{m.onboarding_phases_starting_your_stack()}</h1>
	<p class="lead tabular">
		{m.onboarding_phases_starting_progress({ up: w.started.up, total: w.started.total || "…" })}
	</p>
	<ul class="services" aria-label={m.onboarding_phases_services()}>
		{#each live.filter((s) => !s.deselected && !s.completed_ok) as s (s.name)}
			<li class={s.state}>
				<span class="led" aria-hidden="true"></span>
				<b>{s.name}</b>
				<span class="meta">{stateLabel(s.state)}</span>
			</li>
		{/each}
	</ul>
{/snippet}

{#snippet up()}
	<div class="check" aria-hidden="true"><Check size={26} /></div>
	<h1>{m.onboarding_phases_your_stack_is_up()}</h1>
	{@const jump = aroundSlot((key) => m.onboarding_phases_jump_anywhere({ key }))}
	<p class="lead">
		{m.onboarding_phases_services_running({ count: w.started.total })}
		{jump[0]}<Kbd>{kbd("cmd-k")}</Kbd>{jump[1]}
	</p>
	<div class="actions">
		<Button size="lg" data-testid={ACTIONS.finish} onclick={onfinish}>
			{m.onboarding_phases_start_tour()}
			{#snippet trailing()}<ArrowRight size={15} />{/snippet}
		</Button>
	</div>
{/snippet}

<style>
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
		li,
		.check,
		.services li.starting .led {
			animation: none;
		}
	}
</style>
