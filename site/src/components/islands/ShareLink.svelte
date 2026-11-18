<script lang="ts">
	// "Send me the link" for phones: the native share sheet when the browser has one, a copyable link always, and an email
	// through the same /api/waitlist endpoint (topic "mobile-link") when the visitor prefers that.
	import Waitlist from "./Waitlist.svelte";
	import "../../lib/locale-client";
	import { m } from "../../lib/paraglide/messages.js";
	let { url }: { url: string } = $props();
	const canShare = typeof navigator !== "undefined" && typeof navigator.share === "function";
	let copied = $state(false);
	async function share() {
		try {
			await navigator.share({ title: "Atlas", text: m.islands_share_link_share_text(), url });
		} catch {}
	}
	async function copy() {
		try {
			await navigator.clipboard.writeText(url);
			copied = true;
			setTimeout(() => (copied = false), 1800);
		} catch {}
	}
</script>

<div class="send">
	<div class="row">
		{#if canShare}<button type="button" class="btn solid" onclick={share}>{m.islands_share_link_share_the_link()}</button>{/if}
		<button type="button" class="btn" class:solid={!canShare} onclick={copy}>{copied ? m.islands_share_link_link_copied() : m.islands_share_link_copy_link()}</button>
		<span class="sr-only" role="status">{copied ? m.islands_share_link_link_copied() : ""}</span>
	</div>
	<p class="or">{m.islands_share_link_or_email()}</p>
	<Waitlist variant="close" topic="mobile-link" id="mobile-link" cta={m.islands_share_link_send_me_the_link()} title={m.islands_share_link_got_it()} done={m.islands_share_link_done()} />
</div>

<style>
	.send { display: grid; gap: 0.9rem; min-width: 0; }
	.row { display: flex; flex-wrap: wrap; gap: 0.6rem; }
	.row .btn { flex: 1 1 11rem; }
	.or { margin: 0.4rem 0 0; color: var(--fg-2); font-weight: 600; }
</style>
