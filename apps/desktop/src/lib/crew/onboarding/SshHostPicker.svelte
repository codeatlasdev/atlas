<!--
  Picker of the servers already in the user's ~/.ssh/config, above the free-text SSH address of the connect step.
  The kit's searchable Combobox (arrows, Enter, Esc, type to filter on alias, user, host name and port). Picking one
  hands the host to `onpick`; what it shows as chosen follows the address and port fields, so editing either one by
  hand drops the choice instead of leaving a label that no longer matches.
-->
<script lang="ts">
	import { Combobox } from "@codeatlasdev/ui";
	import { typeInSearchField } from "#lib/crew/searchKeys.ts";
	import { hostLabel, pickedHost, type SshHost } from "#lib/crew/sshHosts.ts";

	let {
		hosts,
		address,
		port,
		disabled = false,
		onpick,
	}: {
		hosts: SshHost[];
		address: string;
		port: number;
		disabled?: boolean;
		onpick: (host: SshHost) => void;
	} = $props();

	const options = $derived(hosts.map((h) => ({ value: h.alias, label: hostLabel(h) })));
	const picked = $derived(pickedHost(hosts, address, port));
</script>

<!-- Printable keys typed in the search field only type (see searchKeys.ts). -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="ssh-host-picker" onkeydowncapture={typeInSearchField}>
	<Combobox
		value={picked?.alias ?? ""}
		{options}
		{disabled}
		label="From your ~/.ssh/config"
		placeholder={`Choose one of ${hosts.length} ${hosts.length === 1 ? "server" : "servers"}`}
		searchPlaceholder="Search your servers"
		emptyText="No server matches: type the address below"
		onchange={(alias) => {
			const h = hosts.find((x) => x.alias === alias);
			if (h) onpick(h);
		}}
	/>
	{#if picked}
		<p class="field-hint">
			Connects the way <code>ssh {picked.alias}</code> does{picked.identity_file
				? `, with ${picked.identity_file}`
				: ""}.
		</p>
	{/if}
</div>

<style>
	.ssh-host-picker {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
</style>
