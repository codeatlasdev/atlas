<script lang="ts">
	import { usePoll, useNow } from "#lib/clock.svelte.ts";
	import { EmptyState, toast } from "@codeatlasdev/ui";
	import { Smartphone } from "@lucide/svelte";
	import { errText } from "#lib/api.ts";
	import { hostCall } from "#lib/host.ts";
	import Group from "#lib/settings/Group.svelte";
	import LoadError from "#lib/settings/LoadError.svelte";
	import PairCard from "#lib/components/mobile/PairCard.svelte";
	import PairHelp from "#lib/components/mobile/PairHelp.svelte";
	import PhoneRow from "#lib/components/mobile/PhoneRow.svelte";
	import {
		deviceTitle,
		secondsLeft,
		sortDevices,
		type Device,
		type PairStart,
	} from "#lib/settings/mobile.ts";
	import { m } from "#lib/i18n/m.ts";

	let devices = $state<Device[]>([]);
	let loaded = $state(false);
	let loadError = $state<string | null>(null);
	let pairing = $state<{ p: PairStart; at: number } | null>(null);
	let starting = $state(false);
	let startError = $state<string | null>(null);
	let justPaired = $state<string | null>(null);
	const clk = useNow(1000);

	async function load() {
		try {
			const prevIds = new Set(devices.filter((d) => !d.revoked).map((d) => d.node_id));
			const next = (await hostCall<{ devices: Device[] }>("mobile_devices")).devices;
			devices = next;
			loadError = null;
			// A phone that was not there when the code was made, and is now: the code did its job.
			const gained = loaded && next.find((d) => !d.revoked && !prevIds.has(d.node_id));
			if (gained && pairing) {
				pairing = null;
				justPaired = deviceTitle(gained);
			}
		} catch (e) {
			loadError = errText(e);
		} finally {
			loaded = true;
		}
	}

	// A live code: look every second, so the phone appears the moment it connects. Always: every few seconds, so
	// "connected now" and "last seen" stay true while the pane is open.
	usePoll(
		1000,
		() => !!pairing && left > 0,
		() => void load(),
	);
	usePoll(
		5000,
		() => true,
		() => void load(),
	);

	async function pair() {
		starting = true;
		startError = null;
		justPaired = null;
		try {
			pairing = { p: await hostCall<PairStart>("mobile_pair_start"), at: Date.now() };
		} catch (e) {
			pairing = null;
			startError = errText(e);
		} finally {
			starting = false;
		}
	}

	const left = $derived(pairing ? secondsLeft(pairing.p, pairing.at, clk.now) : 0);
	const active = $derived(sortDevices(devices.filter((d) => !d.revoked)));
</script>

<Group title={m.settings_agents_mobile_title()} description={m.settings_agents_mobile_description()}>
	<PairCard {pairing} now={clk.now} busy={starting} error={startError} paired={justPaired} onstart={pair} />
	<PairHelp pairing={pairing && left > 0 ? pairing.p : null} />
</Group>

{#if loadError}
	<LoadError what="phones" error={loadError} />
{/if}

<Group title={m.settings_agents_paired_phones()}>
	{#each active as d, i (d.node_id)}
		<PhoneRow
			device={d}
			now={clk.now}
			anchor={i === 0 ? "mobile-devices" : undefined}
			onchange={load}
			onerror={(text) => toast.error(text)}
			ondone={(text) => toast.success(text)}
		/>
	{:else}
		<div data-setting="mobile-devices">
			<EmptyState
				icon={Smartphone}
				title={loaded ? m.settings_agents_mobile_empty_title() : m.settings_agents_mobile_empty_loading()}
				description={loaded ? m.settings_agents_mobile_empty_body() : undefined}
			/>
		</div>
	{/each}
</Group>
