<script lang="ts">
	// `cmd` is shown as is. With `winCmd`, a Windows visitor sees that one instead (the PowerShell installer: `curl | sh` does not run
	// there); the server render and no-JS visitors keep `cmd`, so the page is complete without a script.
	import { onMount } from "svelte";
	import "../../lib/locale-client";
	import { m } from "../../lib/paraglide/messages.js";
	let { cmd, winCmd = "", prompt = "$", label }: { cmd: string; winCmd?: string; prompt?: string; label?: string } = $props();
	let windows = $state(false);
	onMount(() => {
		const nav = navigator as Navigator & { userAgentData?: { platform?: string; mobile?: boolean } };
		const mobile = nav.userAgentData?.mobile ?? /Android|iPhone|iPad|iPod|Mobile/i.test(navigator.userAgent);
		// i18n-ignore: the visitor's OS picks the installer shown (PowerShell on Windows): a download choice on the site, not a key label, and keys.ts is the desktop app's module
		windows = !mobile && /win/i.test(nav.userAgentData?.platform ?? navigator.platform ?? "");
	});
	const shown = $derived(winCmd && windows ? winCmd : cmd);
	const mark = $derived(winCmd && windows ? ">" : prompt);
	let copied = $state(false);
	async function copy() {
		try {
			await navigator.clipboard.writeText(shown);
			copied = true;
			setTimeout(() => (copied = false), 1800);
		} catch {}
	}
</script>

<div class="install">
	<code><span class="p">{mark}</span> {shown}</code>
	<button type="button" onclick={copy} aria-label={winCmd && windows ? m.islands_install_copy_powershell_label() : (label ?? m.islands_install_copy_label())}>{copied ? m.islands_install_copy_copied() : m.islands_install_copy_copy()}</button>
	<span class="sr-only" role="status">{copied ? m.islands_install_copy_status_copied() : ""}</span>
</div>

<style>
	.install { display: flex; align-items: center; gap: 0.8rem; max-width: 100%; min-width: 0; box-sizing: border-box; padding: 0.55rem 0.55rem 0.55rem 1rem; border: 2px solid var(--fg); font-size: clamp(0.78rem, 1.05vw, 0.9rem); color: var(--fg); background: rgb(255 255 255 / 0.08); }
	code { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; scrollbar-width: none; font-weight: 600; }
	code::-webkit-scrollbar { display: none; }
	.p { margin-right: 0.55em; opacity: 0.6; }
	button { flex: none; min-height: 44px; font: 700 0.875rem/1 var(--font); color: var(--ink); background: var(--fg); border: 0; border-radius: 2px; padding: 0.6rem 0.85rem; cursor: pointer; }
	@media (pointer: coarse) { button { min-height: 44px; min-width: 44px; padding-inline: 1rem; } }
	button:hover { background: var(--wait); }
</style>
