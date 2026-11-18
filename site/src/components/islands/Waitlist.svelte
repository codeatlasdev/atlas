<script lang="ts">
	import "../../lib/locale-client";
	import { m } from "../../lib/paraglide/messages.js";
	import { getLocale } from "../../lib/paraglide/runtime.js";
	// Release-notes signup. Contract unchanged: POST /api/waitlist { email, website (honeypot), lang } -> { ok } | { ok:false, error }. Owned by Deploy.
	// `topic` tags the signup (a platform someone waits for, or "mobile-link" for the send-me-the-link form); `cta`, `title`, `done` and `id` customize the copy
	// (messages from the caller); without them the form says its own release-notes text.
	let { variant = "hero", topic = "", cta, title, done, id = "" }: { variant?: "hero" | "paper" | "close"; topic?: string; cta?: string; title?: string; done?: string; id?: string } = $props();
	const uid = $derived(id || variant);
	let email = $state("");
	let website = $state("");
	let status = $state<"idle" | "busy" | "ok" | "error">("idle");
	let message = $state("");

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		if (status === "busy") return;
		status = "busy";
		message = "";
		try {
			const res = await fetch("/api/waitlist", {
				method: "POST",
				headers: { "content-type": "application/json" },
				body: JSON.stringify({ email, website, lang: getLocale(), topic }),
			});
			if (res.ok) {
				status = "ok";
				return;
			}
			const body = await res.json().catch(() => ({}));
			status = "error";
			message =
				body.error === "invalid_email"
					? m.islands_waitlist_invalid_email()
					: body.error === "rate_limited"
						? m.islands_waitlist_rate_limited()
						: m.islands_waitlist_error();
		} catch {
			status = "error";
			message = m.islands_waitlist_unreachable();
		}
	}
</script>

<div class={`wl ${variant}`}>
	{#if status === "ok"}
		<p class="done" role="status"><i class="dot go"></i> <span><b>{title ?? m.islands_waitlist_you_are_on_the_list()}</b> {done ?? m.islands_waitlist_done()}</span></p>
	{:else}
		<form onsubmit={submit} novalidate>
			<label class="sr-only" for={`wl-email-${uid}`}>{m.islands_waitlist_email()}</label>
			<input id={`wl-email-${uid}`} type="email" name="email" inputmode="email" enterkeyhint="send" autocapitalize="none" autocorrect="off" spellcheck="false" required placeholder={m.islands_waitlist_email_placeholder()} autocomplete="email" bind:value={email} aria-describedby={`wl-msg-${uid}`} />
			<input class="hp" type="text" name="website" tabindex="-1" aria-hidden="true" autocomplete="off" bind:value={website} />
			<button class="btn solid" disabled={status === "busy"}>{status === "busy" ? m.islands_waitlist_sending() : (cta ?? m.islands_waitlist_get_release_notes())}</button>
		</form>
		<p id={`wl-msg-${uid}`} class="msg" role="alert">{message}</p>
	{/if}
</div>

<style>
	form { display: flex; gap: 0; flex-wrap: wrap; }
	input[type="email"] { flex: 1 1 14rem; min-width: 0; padding: 0.85rem 1rem; min-height: 48px; border: 2px solid var(--fg); border-right: 0; border-radius: 0; background: transparent; color: var(--fg); font: 500 max(16px, 1rem)/1 var(--mono); -webkit-appearance: none; appearance: none; }
	input[type="email"]::placeholder { color: color-mix(in srgb, var(--fg) 55%, transparent); }
	input[type="email"]:focus { outline: 2px solid var(--wait); outline-offset: 2px; }
	form .btn { border-radius: 0 3px 3px 0; }
	.hp { position: absolute; left: -9999px; width: 1px; height: 1px; opacity: 0; }
	.msg { min-height: 1.3em; margin: 0.55rem 0 0; font-size: 0.92rem; color: var(--wait); font-weight: 600; }
	.paper .msg { color: #b3261e; }
	.done { margin: 0; padding: 0.9rem 1rem; border: 2px solid var(--fg); display: flex; gap: 0.7rem; align-items: baseline; }
	.paper .dot { filter: saturate(0.9) brightness(0.8); }
	@media (max-width: 520px) { input[type="email"] { border-right: 2px solid var(--fg); flex-basis: 100%; border-radius: 3px; } form .btn { border-radius: 3px; margin-top: 0.5rem; width: 100%; min-height: 48px; } .msg { font-size: 1rem; } .done { font-size: 1rem; } }
</style>
