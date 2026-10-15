<script lang="ts">
	import {
		DropdownMenu,
		DropdownMenuItem,
		DropdownMenuLabel,
		DropdownMenuSeparator,
		Kbd,
		Spinner,
		Tooltip,
	} from "@codeatlasdev/ui";
	import { Activity, ChevronDown, Terminal, TriangleAlert, Zap } from "@lucide/svelte";
	import type { ProjectView } from "#lib/api.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { runAction } from "#lib/actions.svelte.ts";
	import Tip from "#lib/components/Tip.svelte";
	import { activity } from "./activity.svelte.ts";
	import { actionOutput } from "./actionOutput.svelte.ts";
	import ActionOutputDrawer from "./ActionOutputDrawer.svelte";
	import ActionResultToast from "./ActionResultToast.svelte";
	import { fold, unread } from "./activity.ts";
	import { buttonState, menuGroups } from "./actions.ts";

	let { project }: { project: ProjectView } = $props();

	let cfg = $derived(store.configs[project.id]);
	let live = $derived(store.live[project.id]);

	let groups = $derived(
		menuGroups(cfg?.actions ?? [], project.running, (k) => !!store.busy[`action:${project.id}:${k}`]),
	);
	let ui = $derived(buttonState(groups.flatMap((g) => g.items)));
	// An action the runner reports as in flight (started elsewhere, e.g. from the TUI) shows too.
	let remote = $derived(live?.operation ?? null);

	let fresh = $derived.by(() => {
		const seen = activity.seen[project.id] ?? 0;
		return unread(fold(live?.events ?? []), seen);
	});
	let badge = $derived(activity.open ? 0 : fresh.n);
	// The menu item stays hidden until an action ran.
	let hasRuns = $derived((live?.runs.rev, !!live?.runs.list.length));
</script>

<Tip label="Activity" command="atlas::ShowActivity">
	{#snippet child(props)}
		<button
			{...props}
			class="ibtn"
			class:on={activity.open}
			aria-label="Activity{badge ? `, ${badge} new` : ''}"
			onclick={() => (activity.open = !activity.open)}
		>
			<Activity size={15} />
			{#if badge}
				<span class="badge" class:err={fresh.errors > 0}>{badge > 9 ? "9+" : badge}</span>
			{/if}
		</button>
	{/snippet}
</Tip>

<ActionOutputDrawer {project} />
<ActionResultToast {project} />

{#if groups.length}
	<DropdownMenu align="end">
		{#snippet triggerChild(props)}
			<button
				{...props}
				class="actions-btn"
				class:busy={!!ui.busy || !!remote}
				aria-label="Actions"
				data-tour="panel-actions"
			>
				{#if ui.busy || remote}
					<!-- The running indicator opens the output; stop the menu from toggling. -->
					<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
					<span
						class="live"
						title="View output"
						onpointerdown={(e) => e.stopPropagation()}
						onclick={(e) => {
							e.stopPropagation();
							actionOutput.show(project.id);
						}}
					>
						<Spinner size={13} />
						<span class="label">{ui.busy?.label ?? remote?.label}</span>
					</span>
				{:else}<Zap size={14} />{/if}
				<ChevronDown size={12} />
			</button>
		{/snippet}
		{#each groups as g, gi (g.label)}
			{#if gi > 0}<DropdownMenuSeparator />{/if}
			{#if g.label}<DropdownMenuLabel>{g.label}</DropdownMenuLabel>{/if}
			{#each g.items as a (a.key)}
				<DropdownMenuItem
					variant={a.confirm ? "destructive" : "default"}
					disabled={!!a.disabled}
					onclick={() => runAction(project.id, a.key)}
				>
					<Tooltip content={a.disabled ?? a.confirm ?? a.label} side="left" delay={250}>
						{#snippet triggerChild(props)}
							<span {...props} class="item">
								{#if a.busy}<Spinner size={13} />{:else if a.confirm}<TriangleAlert size={13} />{:else}<Zap
										size={13}
									/>{/if}
								<span class="name">{a.label}</span>
								<Kbd>{a.key}</Kbd>
							</span>
						{/snippet}
					</Tooltip>
				</DropdownMenuItem>
			{/each}
		{/each}
		{#if hasRuns}
			<DropdownMenuSeparator />
			<DropdownMenuItem onclick={() => actionOutput.show(project.id)}>
				<span class="item">
					<Terminal size={13} />
					<span class="name">View output</span>
				</span>
			</DropdownMenuItem>
		{/if}
	</DropdownMenu>
{/if}

<style>
	.ibtn,
	.actions-btn {
		position: relative;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 4px;
		height: 24px;
		border: 0;
		border-radius: 6px;
		background: none;
		color: var(--ca-text-secondary);
		transition:
			background 120ms ease-out,
			color 120ms ease-out;
	}
	.ibtn {
		width: 26px;
	}
	.actions-btn {
		padding: 0 6px;
		max-width: 220px;
	}
	.ibtn:hover,
	.actions-btn:hover,
	.actions-btn[data-state="open"] {
		color: var(--ca-text-primary);
		background: var(--ca-action-hover);
	}
	.ibtn.on {
		color: var(--space-accent-text, var(--ca-brand-main));
		background: var(--ca-surface-raised);
	}
	.actions-btn.busy {
		color: var(--space-accent-text, var(--ca-brand-main));
	}
	.label {
		font-size: 12px;
		max-width: 130px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.badge {
		position: absolute;
		top: -3px;
		right: -3px;
		min-width: 14px;
		height: 14px;
		padding: 0 3px;
		border-radius: 7px;
		font-size: 9.5px;
		font-weight: 650;
		line-height: 14px;
		text-align: center;
		color: var(--ca-brand-contrast);
		background: var(--space-accent, var(--ca-brand-main));
	}
	.badge.err {
		background: var(--ca-semantic-error);
		color: var(--ca-brand-contrast);
	}
	.live {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		min-width: 0;
	}
	.item {
		display: flex;
		align-items: center;
		gap: 8px;
		flex: 1;
		min-width: 180px;
	}
	.name {
		flex: 1;
	}
</style>
