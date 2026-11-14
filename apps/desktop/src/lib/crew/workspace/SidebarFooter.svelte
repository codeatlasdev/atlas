<script lang="ts">
	/**
	 * SidebarFooter — user avatar and the crew server pill. With no crew server the pill is the "Add a crew server"
	 * call to action; otherwise it opens a click menu (integrations, crew server settings, help).
	 */
	import { Settings, CircleHelp, User, Plug2 } from "@lucide/svelte";
	import { crew } from "#lib/stores/crew.svelte.ts";
	import { githubCrew } from "#lib/crew/githubCrew.svelte.ts";
	import { connectedIntegrationIds } from "#lib/crew/integrations.ts";
	import CrewFlag from "#lib/crew/CrewFlag.svelte";

	let {
		onintegrations,
		onsettings,
		onhelp,
	}: { onintegrations: () => void; onsettings: () => void; onhelp: () => void } = $props();

	let open = $state(false);
	let wrap = $state<HTMLElement>();
	$effect(() => {
		if (!open) return;
		const onDown = (e: PointerEvent) => {
			if (wrap && !wrap.contains(e.target as Node)) open = false;
		};
		const onKey = (e: KeyboardEvent) => {
			if (e.key === "Escape") open = false;
		};
		document.addEventListener("pointerdown", onDown, true);
		document.addEventListener("keydown", onKey);
		return () => {
			document.removeEventListener("pointerdown", onDown, true);
			document.removeEventListener("keydown", onKey);
		};
	});

	const remoteServers = $derived(crew.servers.filter((h) => h.key !== ""));
	const connectedCount = $derived(connectedIntegrationIds(githubCrew.connected).size);

	/** Run a menu action, then close the menu. */
	function pick(action: () => void) {
		action();
		open = false;
	}
</script>

<!-- Footer: user avatar + crew server pill -->
<div class="dm-footer">
	<button class="footer-user-btn" aria-label="User">
		<User size={15} />
	</button>
	<!-- A click menu closes on a click outside or Esc, never on mouseleave: the gap between the pill and the
	     popover closed it while the pointer travelled up to it. -->
	<div class="footer-pill-wrap" bind:this={wrap}>
		{#if remoteServers.length === 0}
			<button class="footer-pill footer-pill--launch" onclick={onhelp} aria-label="Add a crew server">
				<CrewFlag size={13} />
				<span>Add a crew server</span>
			</button>
		{:else}
			<button
				class="footer-pill"
				onclick={() => (open = !open)}
				aria-label={remoteServers[0].name}
				aria-expanded={open}
				aria-haspopup="menu"
			>
				<CrewFlag size={13} />
				<span class="server-dot" class:online={crew.serverOk(remoteServers[0].key)} aria-hidden="true"></span>
				<span class="footer-server-name">{remoteServers[0].name}</span>
			</button>
			{#if open}
				<div class="footer-popover" role="menu">
					<div class="popover-section-title" aria-hidden="true">Integrations</div>
					<button
						class="popover-item popover-item--integration"
						role="menuitem"
						onclick={() => pick(onintegrations)}
						aria-label="Open integrations"
					>
						<Plug2 size={13} aria-hidden="true" />
						<span>Integrations</span>
						{#if connectedCount > 0}
							<span class="integration-status integration-status--on">{connectedCount} connected</span>
						{:else}
							<span class="integration-status">Manage</span>
						{/if}
					</button>
					<div class="popover-divider" role="separator" aria-hidden="true"></div>
					<button class="popover-item" role="menuitem" onclick={() => pick(onsettings)}>
						<Settings size={13} />
						<span>Crew server settings</span>
					</button>
					<button class="popover-item" role="menuitem" onclick={() => pick(onhelp)}>
						<CircleHelp size={13} />
						<span>Help</span>
					</button>
				</div>
			{/if}
		{/if}
	</div>
</div>

<style>
	/* ─── Footer: user + crew server pill ────────────────────────────── */
	.dm-footer {
		flex: none;
		display: flex;
		flex-direction: row;
		align-items: center;
		gap: 8px;
		padding: 8px;
		min-height: 48px;
		border-top: 1px solid var(--ca-border, rgba(255, 255, 255, 0.06));
	}

	.footer-user-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		border-radius: 50%;
		border: none;
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-secondary);
		cursor: default;
		flex: none;
		padding: 0;
	}
	.footer-user-btn:hover {
		color: var(--ca-text-primary);
	}
	.footer-pill-wrap {
		flex: 1;
		min-width: 0;
		position: relative;
	}
	.footer-pill {
		display: flex;
		align-items: center;
		gap: 6px;
		width: 100%;
		padding: 6px 10px;
		border-radius: 999px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: none;
		color: var(--ca-text-secondary);
		font-size: 12px;
		font-weight: 500;
		cursor: default;
		overflow: hidden;
		white-space: nowrap;
		text-overflow: ellipsis;
		transition: background 120ms;
	}
	.footer-pill:hover {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
	}
	.footer-pill--launch {
		background: var(--ca-brand-main);
		color: var(--ca-surface-base);
		height: 40px;
		font-weight: 600;
		border-color: transparent;
	}
	.footer-pill--launch:hover {
		opacity: 0.88;
		color: var(--ca-surface-base);
		background: var(--ca-brand-main);
	}
	.footer-server-name {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.footer-popover {
		position: absolute;
		bottom: calc(100% + 6px);
		left: 0;
		right: 0;
		background: var(--ca-surface-overlay, var(--ca-surface-base));
		border: 0.5px solid var(--ca-outline-subtle);
		border-radius: 10px;
		box-shadow: 0 4px 20px rgba(0, 0, 0, 0.22);
		padding: 4px;
		z-index: 100;
	}
	.popover-item {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 7px 10px;
		border: none;
		background: none;
		border-radius: 7px;
		font-size: 12.5px;
		color: var(--ca-text-primary);
		cursor: default;
		text-align: left;
	}
	.popover-item:hover {
		background: var(--ca-vibrancy-fill);
	}

	.popover-section-title {
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--ca-text-disabled);
		padding: 6px 10px 2px;
	}
	.popover-divider {
		height: 0.5px;
		background: var(--ca-outline-subtle);
		margin: 4px 0;
	}
	.popover-item--integration {
		display: flex;
		align-items: center;
		gap: 7px;
	}
	.integration-status {
		margin-left: auto;
		font-size: 11px;
		color: var(--ca-text-disabled);
	}
	.integration-status--on {
		color: var(--ca-semantic-success, #2f9e44);
	}
</style>
