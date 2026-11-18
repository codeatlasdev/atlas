<script lang="ts">
	import { Badge, Popover } from "@codeatlasdev/ui";
	import { Bell } from "@lucide/svelte";
	import { m } from "#lib/i18n/m.ts";

	export interface InboxItem {
		id: string;
		title: string;
		body?: string;
		time?: string;
		read?: boolean;
		variant?: "success" | "warning";
	}

	let {
		items,
		onRead,
		onReadAll,
		onClear,
		onSettings,
	}: {
		items: InboxItem[];
		onRead: (id: string) => void;
		onReadAll: () => void;
		onClear: () => void;
		onSettings: () => void;
	} = $props();

	let unread = $derived(items.filter((n) => !n.read).length);
</script>

<!--
	The kit's NotificationBell can only show a text line when it is empty ("No notifications"), which tells a first
	user nothing and has no way to the settings (#506). This is the same bell and list on the kit's Popover, with an
	empty state that says what arrives here and where to change it.
-->
<Popover side="bottom" align="end" sideOffset={8}>
	{#snippet triggerChild(props)}
		<button
			{...props}
			class="bell"
			aria-label={unread > 0
				? m.shell_chrome_inbox_bell_unread({ count: unread })
				: m.shell_chrome_notifications()}
		>
			<Badge count={unread || undefined} color="error"
				><Bell size={18} stroke-width={1.8} aria-hidden="true" /></Badge
			>
		</button>
	{/snippet}
	{#snippet children({ close })}
		<div class="panel" role="dialog" aria-label={m.shell_chrome_notifications()}>
			<div class="head">
				<b>{m.shell_chrome_notifications()}</b>
				<span class="acts">
					{#if unread > 0}<button class="act" onclick={onReadAll}
							>{m.shell_chrome_inbox_bell_mark_all_read()}</button
						>{/if}
					{#if items.length}<button class="act" onclick={onClear}>{m.shell_chrome_inbox_bell_clear()}</button
						>{/if}
				</span>
			</div>
			{#if items.length === 0}
				<div class="empty">
					<p class="lead">{m.shell_chrome_inbox_bell_empty_title()}</p>
					<p>
						{m.shell_chrome_inbox_bell_empty_hint()}
					</p>
					<button
						class="act"
						onclick={() => {
							close();
							onSettings();
						}}>{m.shell_chrome_inbox_bell_notification_settings()}</button
					>
				</div>
			{:else}
				<ul>
					{#each items as n (n.id)}
						<li class:unread={!n.read}>
							<button class="item" onclick={() => !n.read && onRead(n.id)}>
								<span class="dot {n.variant ?? ''}" aria-hidden="true"></span>
								<span class="text">
									<span class="title">{n.title}</span>
									{#if n.body}<span class="sub">{n.body}</span>{/if}
									{#if n.time}<span class="sub">{n.time}</span>{/if}
								</span>
							</button>
						</li>
					{/each}
				</ul>
			{/if}
		</div>
	{/snippet}
</Popover>

<style>
	.bell {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 26px;
		border: 0;
		border-radius: 6px;
		background: none;
		color: var(--ca-text-secondary);
		cursor: pointer;
	}
	.bell:hover,
	.bell[data-state="open"] {
		background: var(--ca-action-hover);
		color: var(--ca-text-primary);
	}
	.panel {
		/* The kit's popover pads its content by 16px and caps its width at 320px: cancel the padding so the panel is
		   exactly the popover (a longer pt-BR "Limpar" was cut at the edge). */
		width: 320px;
		margin: -16px;
		max-height: 440px;
		display: flex;
		flex-direction: column;
		font-size: 13px;
	}
	.head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		flex-wrap: wrap;
		gap: 4px 8px;
		padding: 10px 12px 8px;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
	}
	.acts {
		display: flex;
		flex-wrap: wrap;
		justify-content: flex-end;
		gap: 2px 6px;
	}
	.act {
		border: 0;
		background: none;
		padding: 2px 6px;
		border-radius: 4px;
		font-size: 12px;
		color: var(--ca-brand-text);
		cursor: pointer;
		white-space: nowrap;
	}
	.act:hover {
		background: var(--ca-action-hover);
	}
	.empty {
		padding: 22px 16px;
		text-align: center;
		color: var(--ca-text-secondary);
	}
	.empty p {
		margin: 0 0 10px;
		line-height: 1.45;
	}
	.empty .lead {
		color: var(--ca-text-primary);
		font-weight: 600;
		margin-bottom: 6px;
	}
	ul {
		list-style: none;
		margin: 0;
		padding: 6px;
		overflow-y: auto;
	}
	.item {
		display: flex;
		gap: 10px;
		width: 100%;
		padding: 8px;
		border: 0;
		border-radius: 6px;
		background: none;
		font: inherit;
		color: inherit;
		text-align: left;
		cursor: pointer;
	}
	.item:hover {
		background: var(--ca-action-hover);
	}
	.dot {
		flex: none;
		width: 8px;
		height: 8px;
		margin-top: 5px;
		border-radius: 50%;
		background: var(--ca-text-hint);
	}
	li.unread .dot.success {
		background: var(--ca-semantic-success);
	}
	li.unread .dot.warning {
		background: var(--ca-semantic-warning);
	}
	.text {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}
	.title {
		font-weight: 500;
	}
	li:not(.unread) .title {
		font-weight: 400;
		color: var(--ca-text-secondary);
	}
	.sub {
		font-size: 12px;
		color: var(--ca-text-secondary);
		overflow-wrap: anywhere;
	}
</style>
