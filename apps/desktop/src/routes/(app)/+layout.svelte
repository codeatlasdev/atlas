<script lang="ts">
	import "@codeatlasdev/ui/styles/global.css";
	import "@codeatlasdev/ui/styles/materials.css";
	import "../../app.css";
	import CornerStack from "#lib/shell/CornerStack.svelte";
	import { bootMark } from "#lib/boot.ts";
	import { ChevronLeft, ChevronRight } from "@lucide/svelte";
	import { onMount, type Snippet } from "svelte";
	import AskDialog from "#lib/components/AskDialog.svelte";
	import CrashProbe from "#lib/errors/CrashProbe.svelte";
	import Contain from "#lib/errors/Contain.svelte";
	import ErrorToasts from "#lib/errors/ErrorToasts.svelte";
	import LazyOpen from "#lib/components/LazyOpen.svelte";
	import DevLine from "#lib/components/DevLine.svelte";
	import Palette from "#lib/components/Palette.svelte";
	import LspPicker from "#lib/lsp/LspPicker.svelte";
	import Sidebar from "#lib/components/Sidebar.svelte";
	import UpdatedCard from "#lib/components/UpdatedCard.svelte";
	import GhosttyOffer from "#lib/settings/theme/GhosttyOffer.svelte";
	import Onboarding from "#lib/onboarding/Onboarding.svelte";
	import EdgePeek from "#lib/shell/EdgePeek.svelte";
	import TabSwitcher from "#lib/shell/TabSwitcher.svelte";
	import WindowControls from "#lib/shell/WindowControls.svelte";
	import {
		commandItems,
		createItems,
		fileItems,
		jumpItems,
		palettes,
		symbolItems,
	} from "#lib/shell/palettes.svelte.ts";
	import { branchPicker, branchItems, createBranchItems } from "#lib/stores/branch.svelte.ts";
	import { startReadyGate } from "#lib/shell/ready.svelte.ts";
	import { startSession } from "#lib/shell/session.svelte.ts";
	import { shell } from "#lib/shell/shell.svelte.ts";
	import { historySwipe } from "#lib/shell/gestures.ts";
	import { swipe } from "#lib/shell/swipe.ts";
	import { cancelSwitcher } from "#lib/shell/switcher.svelte.ts";
	import { onKeydown, onKeydownCapture, onKeyup } from "#lib/shell/windowKeys.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { docks } from "#lib/stores/docks.svelte.ts";
	import { hostPrompt } from "#lib/stores/hostStatus.svelte.ts";
	import { pr } from "#lib/stores/pr.svelte.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { updates } from "#lib/stores/updates.svelte.ts";
	import { worktrees } from "#lib/stores/worktrees.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import Lazy from "#lib/components/Lazy.svelte";
	import WorkspaceRail from "#lib/shell/WorkspaceRail.svelte";
	import { m } from "#lib/i18n/m.ts";

	let { children }: { children: Snippet } = $props();
	// Windows and Linux: no native title bar (app.html sets the class before first paint).
	const customChrome = document.documentElement.classList.contains("custom-chrome");
	// A tiling WM (Hyprland, Sway, i3) owns move, resize and close: no caption buttons there (#269).
	const captionButtons = customChrome && !document.documentElement.classList.contains("tiling-wm");
	/** The selected space's hue for the chrome outside a layer (see the comment above `.app`). */
	const accent = $derived(shell.active ? shell.accent(shell.active.id) : undefined);
	const tint = $derived(shell.active ? shell.tint(shell.active.id) : undefined);

	// The crash card only exists on the launch after a crash: its store, text and view load only when Rust says there is one.
	let crashPending = $state(false);

	onMount(() => {
		bootMark("layout:mounted");
		void import("#lib/errors/crashCard.svelte.ts").then(async ({ crashCards }) => {
			await crashCards.load();
			crashPending = !!crashCards.card;
		});
		const stopGate = startReadyGate();
		const stopSession = startSession();
		return () => {
			stopGate();
			stopSession();
		};
	});
</script>

<!-- Focus can leave the window while ⌃ is held (no keyup arrives): blur drops the switcher overlay. -->
<svelte:window
	onkeydown={onKeydown}
	onkeydowncapture={onKeydownCapture}
	onkeyup={onKeyup}
	onfocus={() => store.onFocus()}
	onblur={cancelSwitcher}
/>

{#snippet sidebar()}
	<Contain where="Sidebar" title={m.shell_layout_sidebar_title()} hint={m.shell_layout_sidebar_hint()}>
		<Sidebar
			onJump={() => (palettes.jump = true)}
			onAgent={(id) => {
				store.selected = id;
				ws.select(id);
				void agents.launch(id);
			}}
		/>
	</Contain>
{/snippet}

<!--
	Nothing here sets `--space-accent` or `--space-tint` on `.app`. A custom property inherits, so writing it on the root
	restyled every node of the chrome on each selection (~2 ms of the main thread, #452). The leaves that read the
	selection's hue carry it themselves: the `.content` wash and the nav chips below, the edge peek's glass (EdgePeek)
	and the selected worktree row (WorktreeRow). `--space-accent-text` and `--space-on-accent` are read only below a
	layer, and every layer carries all four itself (`shell.layerVars`, SpacePager).
-->
<div class="app">
	<WorkspaceRail />
	{#if ws.appMode === "crew"}
		<!-- The crew workspace loads when it is first opened, not at startup (bundle-budget.json): until it arrives,
		     an empty box of its size and ground holds its place, so the rail does not shift and nothing flashes. -->
		<Lazy load={() => import("#lib/crew/CrewWorkspace.svelte")} props={{}}>
			{#snippet pending()}<div class="crew-pending" aria-hidden="true"></div>{/snippet}
		</Lazy>
	{:else}
		<div class="window-ground" aria-hidden="true"></div>
		<div class="sidebar-slot" class:hidden={!docks.sidebar} inert={!docks.sidebar}>
			<div class="sidebar-panel">{@render sidebar()}</div>
		</div>
		<EdgePeek edge="left" size={260} label={m.shell_layout_sidebar_label()} active={!docks.sidebar} {accent}
			>{@render sidebar()}</EdgePeek
		>
		<main class="content" class:full={!docks.sidebar} use:swipe={historySwipe()}>
			<div class="space-wash" aria-hidden="true" style:--space-tint={tint}></div>
			<div class="space-edge" aria-hidden="true"></div>
			<div class="nav-chip nav-chip-prev" aria-hidden="true" style:--space-accent={accent}>
				<ChevronLeft size={18} />
			</div>
			<div class="nav-chip nav-chip-next" aria-hidden="true" style:--space-accent={accent}>
				<ChevronRight size={18} />
			</div>
			<div class="space-view">
				<CrashProbe id="app" />
				{@render children()}
			</div>
		</main>
	{/if}
</div>

<Palette
	bind:open={palettes.jump}
	placeholder={m.shell_layout_palette_jump()}
	items={jumpItems}
	dynamic={createItems}
/>
<Palette bind:open={palettes.commands} placeholder={m.shell_layout_palette_commands()} items={commandItems} />
<LazyOpen when={worktrees.newDialog.open} load={() => import("#lib/components/NewWorktreeDialog.svelte")} />
<LazyOpen when={!!pr.createFor} load={() => import("#lib/components/CreatePrDialog.svelte")} />
<Palette bind:open={palettes.files} placeholder={m.shell_layout_palette_files()} items={fileItems} ranked />
<Palette
	bind:open={branchPicker.open}
	placeholder={m.shell_layout_palette_branches()}
	items={branchItems}
	dynamic={createBranchItems}
/>
<Palette bind:open={palettes.symbols} placeholder={m.shell_layout_palette_symbols()} items={symbolItems} />
<LspPicker />
<LazyOpen when={updates.open} load={() => import("#lib/components/Settings.svelte")} />
<LazyOpen when={updates.whatsNewOpen} load={() => import("#lib/components/WhatsNew.svelte")} />
<LazyOpen when={hostPrompt.open} load={() => import("#lib/components/HostPrompt.svelte")} />
<UpdatedCard />
<GhosttyOffer />
<DevLine />
{#if ws.appMode !== "crew"}<Onboarding />{/if}
<AskDialog />
<TabSwitcher />
<CornerStack />
<ErrorToasts />
<LazyOpen when={crashPending} load={() => import("#lib/errors/CrashCard.svelte")} />
{#if captionButtons}<WindowControls />{/if}
