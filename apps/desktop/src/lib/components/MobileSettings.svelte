<script lang="ts">
	import { onMount } from "svelte";
	import { Button, Switch, toast } from "@codeatlasdev/ui";
	import { errText } from "#lib/api.ts";
	import { hostCall } from "#lib/host.ts";
	import Group from "#lib/settings/Group.svelte";
	import PageHead from "#lib/settings/PageHead.svelte";
	import Row from "#lib/settings/Row.svelte";
	import PairQr from "#lib/components/PairQr.svelte";
	import { SCOPES, secondsLeft, toggleScope, type Device, type PairStart } from "#lib/settings/mobile.ts";

	let devices = $state<Device[]>([]);
	let pairing = $state<{ p: PairStart; at: number } | null>(null);
	let now = $state(Date.now());

	async function load() {
		const prevIds = new Set(devices.filter((d) => !d.revoked).map((d) => d.node_id));
		devices = (await hostCall<{ devices: Device[] }>("mobile_devices")).devices;
		const gained = devices.find((d) => !d.revoked && !prevIds.has(d.node_id));
		if (gained && pairing) {
			pairing = null;
			toast.success(`Paired ${gained.name}`);
		}
	}

	onMount(() => {
		load().catch((e) => toast.error(errText(e)));
		const t = setInterval(() => {
			now = Date.now();
			// Poll only while a code is live; stop once it expires.
			if (pairing && left > 0) void load().catch(() => {});
		}, 1000);
		return () => clearInterval(t);
	});

	async function pair() {
		try {
			pairing = { p: await hostCall<PairStart>("mobile_pair_start"), at: Date.now() };
		} catch (e) {
			toast.error(errText(e));
		}
	}

	async function setScopes(d: Device, scopes: Device["scopes"]) {
		await hostCall("mobile_set_scopes", { node_id: d.node_id, scopes }).catch((e) => toast.error(errText(e)));
		await load();
	}

	async function revoke(d: Device) {
		await hostCall("mobile_revoke", { node_id: d.node_id }).catch((e) => toast.error(errText(e)));
		await load();
	}

	const left = $derived(pairing ? secondsLeft(pairing.p, pairing.at, now) : 0);
	const active = $derived(devices.filter((d) => !d.revoked));
</script>

<PageHead
	title="Mobile"
	description="See and control this computer's projects, agents and terminals from Atlas Mobile. No account: the phone connects straight to this computer (through an encrypted relay when a direct path is blocked). The computer must be awake."
/>

<Group title="Pair a phone">
	<Row
		id="mobile-pair"
		label="New phone"
		description="Open Atlas Mobile and scan the code. It works once, for two minutes."
	>
		<Button size="sm" onclick={pair}>{pairing && left > 0 ? "New code" : "Pair a phone"}</Button>
		{#snippet below()}
			{#if pairing}
				<PairQr {pairing} {now} />
			{/if}
		{/snippet}
	</Row>
</Group>

<Group title="Paired phones">
	{#each active as d, i (d.node_id)}
		<Row
			id={i === 0 ? "mobile-devices" : undefined}
			label={d.name}
			description={`Paired ${new Date(d.paired_at * 1000).toLocaleDateString()}`}
			stack
		>
			<div class="scopes">
				{#each SCOPES as s (s.id)}
					<label class="scope">
						<Switch
							aria-label={s.label}
							checked={d.scopes.includes(s.id)}
							onchange={(on: boolean) => setScopes(d, toggleScope(d.scopes, s.id, on))}
						/>
						{s.label}
					</label>
				{/each}
			</div>
			<Button size="sm" variant="ghost" onclick={() => revoke(d)}>Revoke</Button>
		</Row>
	{:else}
		<Row id="mobile-devices" label="No phones yet" description="Pair one above." />
	{/each}
</Group>

<style>
	.scopes {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.scope {
		display: flex;
		gap: 8px;
		align-items: center;
		font-size: 12px;
	}
</style>
