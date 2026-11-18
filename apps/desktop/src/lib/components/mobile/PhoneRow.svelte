<script lang="ts">
	/**
	 * One paired phone: who it is, whether it is here, what it may do. The phone-level actions (rename, activity,
	 * unpair) sit in the row header; the permissions fold away under a one-line summary.
	 */
	import { Button, Dialog, TextField } from "@codeatlasdev/ui";
	import FlatDisclosure from "#lib/components/FlatDisclosure.svelte";
	import { Smartphone } from "@lucide/svelte";
	import { errText } from "#lib/api.ts";
	import { hostCall } from "#lib/host.ts";
	import Row from "#lib/settings/Row.svelte";
	import { deviceTitle, isGenericName, type Device, type Scope } from "#lib/settings/mobile.ts";
	import { access, deviceStatus } from "#lib/settings/mobileText.ts";
	import PhoneActivity from "./PhoneActivity.svelte";
	import PhonePermissions from "./PhonePermissions.svelte";
	import { m } from "#lib/i18n/m.ts";

	let {
		device,
		now,
		anchor,
		onchange,
		onerror,
		ondone,
	}: {
		device: Device;
		now: number;
		/** The search id of the first row of the list. */
		anchor?: string;
		/** Something changed on the computer: read the list again. */
		onchange: () => void | Promise<void>;
		onerror: (message: string) => void;
		ondone: (message: string) => void;
	} = $props();

	const title = $derived(deviceTitle(device));
	const level = $derived(access(device.scopes));
	let showActivity = $state(false);
	let renaming = $state(false);
	let draft = $state("");
	let unpairing = $state(false);
	let busy = $state(false);

	async function run(call: Promise<unknown>, done?: string) {
		busy = true;
		try {
			await call;
			if (done) ondone(done);
		} catch (e) {
			onerror(errText(e));
		} finally {
			busy = false;
		}
		await onchange();
	}

	function openRename() {
		draft = isGenericName(device.name) ? "" : device.name;
		renaming = true;
	}

	async function rename() {
		const name = draft.trim();
		if (!name) return;
		renaming = false;
		await run(hostCall("mobile_rename", { node_id: device.node_id, name }));
	}

	async function unpair() {
		unpairing = false;
		await run(hostCall("mobile_revoke", { node_id: device.node_id }), m.mobile_phone_row_unpaired({ title }));
	}

	const setScopes = (scopes: Scope[]) =>
		run(hostCall("mobile_set_scopes", { node_id: device.node_id, scopes }));
</script>

<Row id={anchor} label={title} description={deviceStatus(device, now)}>
	{#snippet lead()}
		<span
			class="icon"
			class:on={device.connected}
			title={device.connected ? m.mobile_phone_row_connected_now() : m.mobile_phone_row_not_connected()}
		>
			<Smartphone size={18} />
		</span>
	{/snippet}
	<Button size="sm" variant="ghost" tone="neutral" onclick={openRename}>{m.common_action_rename()}</Button>
	<Button
		size="sm"
		variant="ghost"
		tone="neutral"
		aria-expanded={showActivity}
		onclick={() => (showActivity = !showActivity)}>{m.mobile_phone_row_activity()}</Button
	>
	<Button size="sm" variant="ghost" tone="neutral" onclick={() => (unpairing = true)}
		>{m.mobile_phone_row_unpair()}</Button
	>
	{#snippet below()}
		<div class="access">
			<span class="level" data-kind={level.kind}>{level.label}</span>
			<span class="detail">{level.detail}</span>
		</div>
		<FlatDisclosure title={m.mobile_phone_row_edit_permissions()}>
			<PhonePermissions scopes={device.scopes} onchange={setScopes} />
		</FlatDisclosure>
		{#if showActivity}
			<PhoneActivity nodeId={device.node_id} {now} />
		{/if}
	{/snippet}
</Row>

<Dialog
	bind:open={renaming}
	title={m.mobile_phone_row_name_title()}
	description={m.mobile_phone_row_name_description()}
	size="sm"
>
	<form
		onsubmit={(e) => {
			e.preventDefault();
			void rename();
		}}
	>
		<TextField
			label={m.mobile_phone_row_name_label()}
			bind:value={draft}
			placeholder={m.mobile_phone_row_name_placeholder()}
			maxlength={64}
			autofocus
		/>
		<button type="submit" hidden aria-hidden="true"></button>
	</form>
	{#snippet actions()}
		<Button variant="ghost" onclick={() => (renaming = false)}>{m.common_action_cancel()}</Button>
		<Button disabled={busy || draft.trim() === ""} onclick={rename}>{m.common_action_save()}</Button>
	{/snippet}
</Dialog>

<!-- A body, not only a description: a kit Dialog with no children puts its text against the footer rule
     (codeatlasdev/ui#1329). -->
<Dialog bind:open={unpairing} title={m.mobile_phone_row_unpair_title({ title })} size="sm">
	<p class="consequence">
		{m.mobile_phone_row_unpair_body()}
	</p>
	{#snippet actions()}
		<Button variant="ghost" onclick={() => (unpairing = false)}>{m.common_action_cancel()}</Button>
		<Button variant="destructive" disabled={busy} onclick={unpair}>{m.mobile_phone_row_unpair()}</Button>
	{/snippet}
</Dialog>

<style>
	.icon {
		position: relative;
		display: grid;
		place-items: center;
		width: 28px;
		height: 28px;
		border-radius: 8px;
		background: var(--ca-surface-sunken);
		color: var(--ca-text-secondary);
	}
	.icon.on {
		color: var(--ca-text-primary);
	}
	.icon.on::after {
		content: "";
		position: absolute;
		right: -2px;
		bottom: -2px;
		width: 9px;
		height: 9px;
		border-radius: 50%;
		background: var(--ca-success-main, #30d158);
		box-shadow: 0 0 0 2px var(--ca-surface-raised);
	}
	.consequence {
		margin: 0;
		font-size: 13px;
		line-height: 1.5;
		color: var(--ca-text-secondary);
	}
	.access {
		display: flex;
		align-items: baseline;
		gap: 8px;
		flex-wrap: wrap;
		margin-bottom: 4px;
	}
	.level {
		font-size: 11.5px;
		font-weight: 600;
		padding: 2px 8px;
		border-radius: 999px;
		background: var(--ca-surface-sunken);
		color: var(--ca-text-primary);
	}
	.level[data-kind="full"] {
		background: color-mix(in oklab, var(--ca-warning-main, #ff9f0a) 18%, transparent);
	}
	.detail {
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
</style>
