<script lang="ts">
	import { Button, Spinner, Tooltip, toast } from "@codeatlasdev/ui";
	import { Check, CircleAlert, Download, Info, RotateCw } from "@lucide/svelte";
	import { listen } from "@tauri-apps/api/event";
	import { onMount } from "svelte";
	import { api, errText, type SetupConfig, type SetupStatus } from "#lib/api.ts";
	import { css, parseAnsi } from "#lib/ansi.ts";

	let { id, setup }: { id: string; setup: SetupConfig } = $props();

	/** While a run is in flight the log grows on disk only: re-read it this often. */
	const POLL_MS = 1000;
	/** Lines of live output shown while running or after a failure. */
	const TAIL = 6;

	let info = $state<SetupStatus | null>(null);
	let full = $state(false);
	let starting = $state(false);
	let now = $state(Date.now());
	let pre = $state<HTMLElement | null>(null);

	let st = $derived(info?.status ?? null);
	let running = $derived(starting || st?.state === "running");
	let failed = $derived(!running && st?.state === "failed");
	let lines = $derived(info?.output ? info.output.split("\n") : []);
	// Expanded on its own when it needs attention; the full output is opt-in otherwise.
	let showLines = $derived(full ? lines : running || failed ? lines.slice(-TAIL) : []);

	let schedule = $derived(
		setup.when === "always"
			? "Runs before every start."
			: "Runs when the lockfile changes (or node_modules is missing).",
	);
	let origin = $derived(
		setup.auto ? "Detected from your project; no pre_command in atlas.yaml." : "From pre_command.",
	);

	async function refresh() {
		try {
			info = await api.setupStatus(id);
		} catch (e) {
			toast.error(errText(e));
		}
	}

	async function run() {
		starting = true;
		try {
			const res = await api.setupRun(id);
			if (!res.ok) toast.error(`${setup.command} failed: ${res.reason ?? "unknown error"}`, 8000);
			else toast.success(res.skipped ? "Nothing to run" : "Dependencies installed");
		} catch (e) {
			toast.error(errText(e), 8000);
		} finally {
			starting = false;
			await refresh();
		}
	}

	/** `3m ago` from a unix-seconds timestamp. */
	function ago(secs: number): string {
		const d = Math.max(0, Math.round(now / 1000 - secs));
		if (d < 60) return `${d}s ago`;
		if (d < 3600) return `${Math.floor(d / 60)}m ago`;
		if (d < 86_400) return `${Math.floor(d / 3600)}h ago`;
		return `${Math.floor(d / 86_400)}d ago`;
	}

	// Re-read when the project changes or `projects://setup` fires; tick the clock for "x ago".
	$effect(() => {
		void id;
		refresh();
	});

	// Keep the newest output in view while it streams.
	$effect(() => {
		void lines.length;
		if (pre && running) pre.scrollTop = pre.scrollHeight;
	});

	// Live tail: poll the log while it runs (the stack's own start also runs setup).
	$effect(() => {
		if (!running) return;
		const t = setInterval(refresh, POLL_MS);
		return () => clearInterval(t);
	});

	onMount(() => {
		const tick = setInterval(() => (now = Date.now()), 15_000);
		const off = listen<string>("projects://setup", (e) => e.payload === id && refresh());
		return () => {
			clearInterval(tick);
			off.then((f) => f());
		};
	});
</script>

{#if setup.command}
	<div class="strip" class:running class:failed data-setup={running ? "running" : (st?.state ?? "never")}>
		<div class="head">
			<span class="ico">
				{#if running}<Spinner size={13} />{:else if failed}<CircleAlert size={14} />{:else if st}<Check
						size={14}
					/>{:else}<Download size={13} />{/if}
			</span>
			<span class="title">Setup</span>
			<code class="cmd mono" title={setup.command}>{setup.command}</code>
			<Tooltip content="{schedule} {origin}" side="bottom" delay={200}>
				{#snippet triggerChild(props)}
					<button {...props} class="info" aria-label="When does setup run? {schedule}">
						<Info size={12} />
					</button>
				{/snippet}
			</Tooltip>
			<span class="sep">·</span>
			<span class="state">
				{#if running}running…{:else if failed}failed {ago(
						st?.finished_at ?? st?.started_at ?? 0,
					)}{:else if st}ran {ago(st.finished_at ?? st.started_at)} · ok{:else}not run yet{/if}
			</span>
			<span class="grow"></span>
			{#if lines.length && !running && !failed}
				<button class="link" aria-expanded={full} onclick={() => (full = !full)}>
					{full ? "Hide output" : "View output"}
				</button>
			{:else if lines.length && (running || failed)}
				<button class="link" aria-expanded={full} onclick={() => (full = !full)}>
					{full ? "Less" : "Full output"}
				</button>
			{/if}
			{#if failed}
				<Button size="sm" onclick={run}>
					{#snippet leading()}<RotateCw size={13} />{/snippet}
					Retry
				</Button>
			{:else}
				<button class="link" disabled={running} onclick={run}>{st ? "Run again" : "Run now"}</button>
			{/if}
		</div>
		{#if failed && st?.reason}<div class="reason">{st.reason}</div>{/if}
		{#if showLines.length}
			<pre
				class="out mono"
				class:tail={!full}
				bind:this={pre}>{#each showLines as l, i (i)}{#each parseAnsi(l) as r, j (j)}<span
							style={css(r.style)}>{r.text}</span
						>{/each}{"\n"}{/each}</pre>
		{/if}
	</div>
{/if}

<style>
	.strip {
		border-radius: 10px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-raised);
		padding: 0 12px;
		transition:
			border-color 160ms ease-out,
			background 160ms ease-out;
	}
	.strip.running {
		border-color: color-mix(in oklab, var(--ca-semantic-warning) 45%, transparent);
		padding-bottom: 8px;
	}
	.strip.failed {
		border-color: color-mix(in oklab, var(--ca-semantic-error) 55%, transparent);
		background: color-mix(in oklab, var(--ca-semantic-error) 7%, var(--ca-surface-raised));
		padding-bottom: 8px;
	}
	.head {
		display: flex;
		align-items: center;
		gap: 8px;
		min-height: 34px;
		min-width: 0;
		font-size: 12px;
	}
	.ico {
		display: inline-flex;
		color: var(--ca-semantic-success);
		flex: none;
	}
	.running .ico {
		color: var(--ca-semantic-warning);
	}
	.failed .ico {
		color: var(--ca-semantic-error);
	}
	.title {
		font-weight: 650;
		flex: none;
	}
	.cmd {
		font-size: 11.5px;
		color: var(--ca-text-secondary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		min-width: 0;
	}
	.info {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 18px;
		height: 18px;
		border: 0;
		padding: 0;
		border-radius: 5px;
		background: none;
		color: var(--ca-text-disabled);
		flex: none;
	}
	.info:hover {
		color: var(--ca-text-primary);
		background: var(--ca-action-hover);
	}
	.sep {
		color: var(--ca-text-disabled);
		flex: none;
	}
	.state {
		color: var(--ca-text-secondary);
		white-space: nowrap;
	}
	.failed .state {
		color: var(--ca-semantic-error);
		font-weight: 600;
	}
	.running .state {
		color: var(--ca-semantic-warning);
	}
	.grow {
		flex: 1;
	}
	.link {
		border: 0;
		padding: 2px 4px;
		border-radius: 5px;
		background: none;
		font: inherit;
		font-size: 11.5px;
		color: var(--ca-text-secondary);
		white-space: nowrap;
	}
	.link:hover:not(:disabled) {
		color: var(--ca-text-primary);
		background: var(--ca-action-hover);
	}
	.link:disabled {
		color: var(--ca-text-disabled);
	}
	.reason {
		font-size: 12px;
		font-weight: 550;
		color: var(--ca-semantic-error);
		padding: 0 0 6px 21px;
		word-break: break-word;
	}
	.out {
		margin: 0;
		padding: 8px 10px;
		max-height: 260px;
		overflow: auto;
		font-size: 11.5px;
		line-height: 1.45;
		border-radius: 8px;
		background: var(--ca-surface-sunken);
		white-space: pre-wrap;
		word-break: break-all;
	}
	.out.tail {
		max-height: 130px;
	}
	.strip:not(.running):not(.failed) .out {
		margin-bottom: 8px;
	}
</style>
