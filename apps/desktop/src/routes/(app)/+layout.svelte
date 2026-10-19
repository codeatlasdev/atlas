<script lang="ts">
	import "@codeatlasdev/ui/styles/global.css";
	import "@codeatlasdev/ui/styles/materials.css";
	import "../../app.css";
	import "@xterm/xterm/css/xterm.css";
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

	let { children }: { children: Snippet } = $props();

	onMount(() => {
		bootMark("layout:mounted");
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
	<Contain where="Sidebar" title="The sidebar hit an error" hint="The workspace to the right still works.">
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
	Only what the shell chrome outside a layer reads (sidebar rows, the edge peek, the `.content` wash, the nav chips):
	`--space-accent` and `--space-tint`. `--space-accent-text` and `--space-on-accent` are read only below a layer
	(panel, project header, editor, viewers), and every layer carries all four itself (`shell.layerVars`, SpacePager).
-->
<div
	class="app"
	style:--space-accent={shell.active ? shell.accent(shell.active.id) : undefined}
	style:--space-tint={shell.active ? shell.tint(shell.active.id) : undefined}
>
	<div class="sidebar-slot" class:hidden={!docks.sidebar} inert={!docks.sidebar}>
		<div class="sidebar-panel">{@render sidebar()}</div>
	</div>
	<EdgePeek edge="left" size={260} label="Sidebar" active={!docks.sidebar}>{@render sidebar()}</EdgePeek>
	<main class="content" class:full={!docks.sidebar} use:swipe={historySwipe()}>
		<div class="space-edge" aria-hidden="true"></div>
		<div class="nav-chip nav-chip-prev" aria-hidden="true"><ChevronLeft size={18} /></div>
		<div class="nav-chip nav-chip-next" aria-hidden="true"><ChevronRight size={18} /></div>
		<div class="space-view">
			<CrashProbe id="app" />
			{@render children()}
		</div>
	</main>
</div>

<Palette
	bind:open={palettes.jump}
	placeholder="Jump to worktree, terminal or command"
	items={jumpItems}
	dynamic={createItems}
/>
<Palette bind:open={palettes.commands} placeholder="Run a command" items={commandItems} />
<LazyOpen when={worktrees.newDialog.open} load={() => import("#lib/components/NewWorktreeDialog.svelte")} />
<LazyOpen when={!!pr.createFor} load={() => import("#lib/components/CreatePrDialog.svelte")} />
<Palette bind:open={palettes.files} placeholder="Go to file" items={fileItems} />
<Palette
	bind:open={branchPicker.open}
	placeholder="Switch branch or create one"
	items={branchItems}
	dynamic={createBranchItems}
/>
<Palette bind:open={palettes.symbols} placeholder="Go to symbol in file" items={symbolItems} />
<LspPicker />
<LazyOpen when={updates.open} load={() => import("#lib/components/Settings.svelte")} />
<LazyOpen when={updates.whatsNewOpen} load={() => import("#lib/components/WhatsNew.svelte")} />
<LazyOpen when={hostPrompt.open} load={() => import("#lib/components/HostPrompt.svelte")} />
<UpdatedCard />
<GhosttyOffer />
<DevLine />
<Onboarding />
<AskDialog />
<TabSwitcher />
<CornerStack />
<ErrorToasts />
