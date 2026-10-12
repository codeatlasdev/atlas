<script lang="ts">
	let { cmd }: { cmd: string } = $props();
	let copied = $state(false);
	async function copy() {
		try {
			await navigator.clipboard.writeText(cmd);
			copied = true;
			setTimeout(() => (copied = false), 1800);
		} catch {}
	}
</script>

<div class="install">
	<code><span class="p">$</span> {cmd}</code>
	<button type="button" onclick={copy} aria-label="Copy install command">{copied ? "copied" : "copy"}</button>
	<span class="sr-only" role="status">{copied ? "Install command copied" : ""}</span>
</div>

<style>
	.install { display: flex; align-items: center; gap: 0.8rem; max-width: 100%; min-width: 0; box-sizing: border-box; padding: 0.55rem 0.55rem 0.55rem 1rem; border: 2px solid var(--fg); font-size: clamp(0.78rem, 1.05vw, 0.9rem); color: var(--fg); background: rgb(255 255 255 / 0.08); }
	code { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; scrollbar-width: none; font-weight: 600; }
	code::-webkit-scrollbar { display: none; }
	.p { margin-right: 0.55em; opacity: 0.6; }
	button { flex: none; font: 700 0.875rem/1 var(--font); color: var(--ink); background: var(--fg); border: 0; border-radius: 2px; padding: 0.6rem 0.85rem; cursor: pointer; }
	@media (pointer: coarse) { button { min-height: 44px; min-width: 44px; padding-inline: 1rem; } }
	button:hover { background: var(--wait); }
</style>
