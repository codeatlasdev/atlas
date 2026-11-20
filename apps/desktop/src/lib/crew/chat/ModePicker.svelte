<script lang="ts">
	/**
	 * ModePicker — the permission-mode control under the composer: a chip with the current mode that opens a menu of
	 * the modes the crew allows (Bypass is hidden while the crew forbids it). A switch is `crewmate_set_approval`; a
	 * refusal is hostd's coded error in the app's language. An engine without an approval channel shows its mode
	 * fixed, with the reason next to it. While the crewmate works, the menu and a toast say the change applies from the
	 * next tool call.
	 */
	import { Check, ChevronDown } from "@lucide/svelte";
	import { DropdownMenu, DropdownMenuItem, DropdownMenuLabel, toast } from "@codeatlasdev/ui";
	import { errText } from "#lib/api.ts";
	import type { Crewmate, PermissionMode } from "#lib/crew.ts";
	import ModeBadge from "./ModeBadge.svelte";
	import { modeHint, modeLabel, modeLocked, modeOf, selectableModes } from "#lib/crew/permissionMode.ts";
	import { crew } from "#lib/stores/crew.svelte.ts";
	import { m } from "#lib/i18n/m.ts";

	let { crewmate, hostKey }: { crewmate: Crewmate; hostKey: string } = $props();

	const mode = $derived(modeOf(crewmate));
	const locked = $derived(modeLocked(crewmate));
	const modes = $derived(selectableModes(crew.policyOf(hostKey)));
	const running = $derived(crewmate.state !== "idle");
	let pending = $state(false);

	async function pick(next: PermissionMode) {
		if (pending || next === mode) return;
		pending = true;
		const wasRunning = running;
		try {
			await crew.setMode(hostKey, crewmate.id, next);
			if (wasRunning) toast.info(m.crew_permission_applies_next());
		} catch (e) {
			toast.error(errText(e));
		} finally {
			pending = false;
		}
	}
</script>

<div class="mode-picker">
	{#if locked}
		<span class="chip fixed"><ModeBadge {mode} size={13} /></span>
		<span class="reason">{m.crew_permission_fixed_reason()}</span>
	{:else}
		<DropdownMenu align="start">
			{#snippet triggerChild(props)}
				<button
					type="button"
					{...props}
					class="chip"
					class:amber={mode === "bypass"}
					disabled={pending}
					aria-label={`${m.crew_permission_picker_label({ name: crewmate.name })}: ${modeLabel(mode)}`}
				>
					<ModeBadge {mode} size={13} />
					<ChevronDown size={12} aria-hidden="true" />
				</button>
			{/snippet}
			<DropdownMenuLabel>{m.crew_permission_picker_menu_label()}</DropdownMenuLabel>
			{#each modes as option (option)}
				<DropdownMenuItem onclick={() => void pick(option)}>
					<span class="option">
						<span class="option-top">
							<ModeBadge mode={option} size={13} />
							{#if option === mode}<Check size={12} aria-hidden="true" />{/if}
						</span>
						<span class="option-hint">{modeHint(option)}</span>
					</span>
				</DropdownMenuItem>
			{/each}
			{#if running}
				<DropdownMenuLabel>{m.crew_permission_applies_next()}</DropdownMenuLabel>
			{/if}
		</DropdownMenu>
	{/if}
</div>

<style>
	.mode-picker {
		display: flex;
		align-items: center;
		gap: 8px;
		min-width: 0;
	}
	.chip {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 3px 8px;
		border-radius: 999px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: none;
		color: var(--ca-text-secondary);
		font: inherit;
		cursor: default;
	}
	button.chip:hover,
	button.chip:focus-visible {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
		outline: none;
	}
	button.chip:focus-visible {
		outline: 2px solid color-mix(in oklab, var(--crew, var(--ca-brand-main)) 60%, transparent);
		outline-offset: 1px;
	}
	.chip.amber {
		border-color: color-mix(in oklab, var(--ca-semantic-warning) 45%, transparent);
	}
	.chip:disabled {
		opacity: 0.6;
	}
	.chip.fixed {
		border-style: dashed;
	}
	.reason {
		font-size: 11.5px;
		color: var(--ca-text-secondary);
	}
	.option {
		display: flex;
		flex-direction: column;
		gap: 2px;
		max-width: 280px;
		white-space: normal;
	}
	.option-top {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		color: var(--ca-text-primary);
	}
	.option-hint {
		font-size: 11.5px;
		color: var(--ca-text-secondary);
	}
</style>
