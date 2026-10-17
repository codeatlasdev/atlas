<script lang="ts">
	import { onMount } from "svelte";
	import { invoke } from "@tauri-apps/api/core";
	import { Button, Switch, toast } from "@codeatlasdev/ui";
	import { renderSVG } from "uqr";
	import { errText } from "#lib/api.ts";
	import Group from "#lib/settings/Group.svelte";
	import PageHead from "#lib/settings/PageHead.svelte";
	import Row from "#lib/settings/Row.svelte";
	import {
		pairUrl,
		SCOPES,
		secondsLeft,
		toggleScope,
		type Device,
		type PairStart,
	} from "#lib/settings/mobile.ts";

	let devices = $state<Device[]>([]);
	let pairing = $state<{ p: PairStart; at: number } | null>(null);
	let now = $state(Date.now());

	const call = <T,>(cmd: string, args: Record<string, unknown> = {}) => invoke<T>("host_call", { cmd, args });

	async function load() {
		devices = (await call<{ devices: Device[] }>("mobile_devices")).devices;
	}

	onMount(() => {
		load().catch((e) => toast.error(errText(e)));
		const t = setInterval(() => {
			now = Date.now();
			// A paired phone shows up while the code is on screen.
			if (pairing) void load().catch(() => {});
		}, 1000);
		return () => clearInterval(t);
	});

	async function pair() {
		try {
			pairing = { p: await call<PairStart>("mobile_pair_start"), at: Date.now() };
		} catch (e) {
			toast.error(errText(e));
		}
	}

	async function setScopes(d: Device, scopes: Device["scopes"]) {
		await call("mobile_set_scopes", { node_id: d.node_id, scopes }).catch((e) => toast.error(errText(e)));
		await load();
	}

	async function revoke(d: Device) {
		await call("mobile_revoke", { node_id: d.node_id }).catch((e) => toast.error(errText(e)));
		await load();
	}

	const left = $derived(pairing ? secondsLeft(pairing.p, pairing.at, now) : 0);
	const active = $derived(devices.filter((d) => !d.revoked));
</script>

<PageHead
	title="Mobile"
	description="See and control this Mac's projects, agents and terminals from Atlas Mobile. No account: the phone connects straight to this Mac (through an encrypted relay when a direct path is blocked). The Mac must be awake."
/>

<Group title="Pair a phone">
	<Row
		id="mobile-pair"
		label="New phone"
		description="Open Atlas Mobile and scan the code. It works once, for two minutes."
	>
		<Button size="sm" onclick={pair}>{pairing && left > 0 ? "New code" : "Pair a phone"}</Button>
		{#snippet below()}
			{#if pairing && left > 0}
				<div class="pair">
					<div class="qr" aria-label="Pairing QR code" role="img">{@html renderSVG(pairUrl(pairing.p))}</div>
					<div>
						<div class="code ca-selectable">{pairing.p.code}</div>
						<div class="muted">Expires in {left}s</div>
					</div>
				</div>
			{:else if pairing}
				<div class="muted">The code expired. Make a new one.</div>
			{/if}
		{/snippet}
	</Row>
</Group>

<Group title="Paired phones">
	{#each active as d (d.node_id)}
		<Row label={d.name} description={`Paired ${new Date(d.paired_at * 1000).toLocaleDateString()}`} stack>
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
	.pair {
		display: flex;
		gap: 16px;
		align-items: center;
		margin-top: 8px;
	}
	.qr {
		width: 180px;
		height: 180px;
		background: #fff;
		padding: 8px;
		border-radius: 8px;
	}
	.qr :global(svg) {
		width: 100%;
		height: 100%;
	}
	.code {
		font: 600 22px/1 var(--ca-font-mono, monospace);
		letter-spacing: 0.15em;
	}
	.muted {
		font-size: 11.5px;
		opacity: 0.7;
	}
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
