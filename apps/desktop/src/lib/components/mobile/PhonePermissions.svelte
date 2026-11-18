<script lang="ts">
	/** What one phone may do: View and Control groups, each permission with the risk of granting it. */
	import { SettingItem, Switch } from "@codeatlasdev/ui";
	import { toggleScope, type Scope } from "#lib/settings/mobile.ts";
	import { scopeGroups } from "#lib/settings/mobileText.ts";

	let { scopes, onchange }: { scopes: Scope[]; onchange: (next: Scope[]) => void } = $props();
	const groups = $derived(scopeGroups());
</script>

<div class="perms">
	{#each groups as g (g.title)}
		<section>
			<h4>{g.title}</h4>
			{#each g.scopes as s (s.id)}
				<SettingItem label={s.label} description={s.risk}>
					<Switch
						checked={scopes.includes(s.id)}
						onchange={(on: boolean) => onchange(toggleScope(scopes, s.id, on))}
					/>
				</SettingItem>
			{/each}
		</section>
	{/each}
</div>

<style>
	.perms {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}
	h4 {
		margin: 0;
		font-size: 11px;
		font-weight: 650;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--ca-text-secondary);
	}
	section :global(.ca-setting-item) {
		min-height: 0;
		padding: 7px 0;
	}
	section :global(.ca-setting-item + .ca-setting-item) {
		border-top: 0.5px solid var(--ca-outline-subtle);
	}
</style>
