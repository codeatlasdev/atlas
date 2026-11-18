<script lang="ts">
	import { Eye, EyeOff, TriangleAlert, FileDiff, Search } from "@lucide/svelte";
	import { host, b64bytes } from "#lib/host.ts";
	import { diffEnv, ENV_EXAMPLES, looksSecret, parseEnv, type EnvEntry } from "#lib/viewers/rich/env.ts";
	import type { RichCtx } from "#lib/viewers/rich/ctx.ts";
	import Copy from "#lib/viewers/rich/Copy.svelte";
	import "#lib/viewers/rich/rich.css";
	import { m } from "#lib/i18n/m.ts";

	let { ctx }: { ctx: RichCtx } = $props();

	let doc = $derived(parseEnv(ctx.text));
	let revealed = $state<Record<number, boolean>>({});
	let all = $state(false);
	let q = $state("");
	let example = $state<{ name: string; text: string } | null>(null);

	let vars = $derived(doc.entries.filter((e): e is Extract<EnvEntry, { kind: "var" }> => e.kind === "var"));
	let invalid = $derived(
		doc.entries.filter((e): e is Extract<EnvEntry, { kind: "invalid" }> => e.kind === "invalid"),
	);
	let diff = $derived(example ? diffEnv(doc, parseEnv(example.text)) : null);
	let rows = $derived(
		doc.entries.filter((e) => {
			if (!q) return true;
			const s = q.toLowerCase();
			return e.kind === "var"
				? e.key.toLowerCase().includes(s)
				: e.kind === "comment" && e.text.toLowerCase().includes(s);
		}),
	);

	// The sibling `.env.example` (or sample/template/dist) to compare against, unless this file is that file.
	$effect(() => {
		const dir = ctx.path.split("/").slice(0, -1).join("/");
		const self = ctx.path.split("/").pop()!;
		example = null;
		let live = true;
		void (async () => {
			for (const name of ENV_EXAMPLES) {
				if (name === self) continue;
				try {
					const r = await host.fsRead(`${ctx.root}/${dir ? dir + "/" : ""}${name}`, 256 * 1024);
					if (live) example = { name, text: new TextDecoder().decode(b64bytes(r.data)) };
					return;
				} catch {
					// Not there: try the next convention.
				}
			}
		})();
		return () => (live = false);
	});

	const shown = (e: { line: number }) => all || revealed[e.line];
	const mask = (v: string) => "•".repeat(Math.min(Math.max(v.length, 6), 24));
	const dup = (k: string) => doc.duplicates.has(k);
</script>

<div class="rv">
	<div class="rv-inner">
		<header class="rv-head">
			<div class="rv-title">
				<h1>{ctx.path.split("/").pop()}</h1>
				<span class="rv-ver">{m.viewers_devops_env_view_variables_count({ count: vars.length })}</span>
			</div>
			<div class="rv-facts">
				{#if doc.duplicates.size}<span class="rv-chip warn"
						>{m.viewers_devops_env_view_duplicated({ count: doc.duplicates.size })}</span
					>{/if}
				{#if invalid.length}<span class="rv-chip bad"
						>{m.viewers_devops_env_view_error_lines_count({ count: invalid.length })}</span
					>{/if}
				{#if diff && !diff.missing.length && !diff.extra.length}
					<span class="rv-chip ok">{m.viewers_devops_env_view_matches({ name: example?.name ?? "" })}</span>
				{/if}
			</div>
		</header>

		{#if diff && (diff.missing.length || diff.extra.length)}
			<section class="rv-sec">
				<h2>
					<FileDiff size={13} />
					{m.viewers_devops_env_view_compared_with({ name: example?.name ?? "" })}
				</h2>
				<div class="rv-list">
					{#if diff.missing.length}
						<div class="rv-row warn">
							<span class="rv-chip warn">{m.viewers_devops_env_view_missing()}</span>
							<span class="rv-v wrap">{diff.missing.join("  ")}</span>
							<span class="rv-note">{m.viewers_devops_env_view_only_in_example()}</span>
						</div>
					{/if}
					{#if diff.extra.length}
						<div class="rv-row">
							<span class="rv-chip info">{m.viewers_devops_env_view_extra()}</span>
							<span class="rv-v wrap">{diff.extra.join("  ")}</span>
							<span class="rv-note">{m.viewers_devops_env_view_only_here()}</span>
						</div>
					{/if}
				</div>
			</section>
		{/if}

		<section class="rv-sec">
			<h2>
				{m.viewers_devops_variables()}
				<span class="sp"></span>
				<label class="rv-search">
					<Search size={12} />
					<input
						placeholder={m.viewers_devops_filter()}
						bind:value={q}
						aria-label={m.viewers_devops_env_view_filter_variables()}
					/>
				</label>
				<button class="rv-btn" class:on={all} onclick={() => (all = !all)} aria-pressed={all}>
					{#if all}<EyeOff size={12} /> {m.viewers_devops_env_view_hide_values()}{:else}<Eye size={12} />
						{m.viewers_devops_env_view_reveal_all()}{/if}
				</button>
			</h2>
			<div class="rv-list">
				{#each rows as e (e.line)}
					{#if e.kind === "var"}
						<div class="rv-row" class:warn={dup(e.key)}>
							<span class="rv-k">{e.key}</span>
							<span
								class="rv-v"
								class:wrap={shown(e)}
								class:masked={!shown(e)}
								title={shown(e) ? e.value : ""}
							>
								{#if e.value === ""}<i class="empty">{m.viewers_devops_env_view_empty()}</i
									>{:else if shown(e)}{e.value}{:else}{mask(e.value)}{/if}
							</span>
							{#if e.comment}<span class="rv-note">{e.comment}</span>{/if}
							{#if dup(e.key)}
								{@const others = doc.duplicates.get(e.key)?.filter((l) => l !== e.line) ?? []}
								<span
									class="rv-chip warn"
									title={m.viewers_devops_env_view_also_defined({
										count: others.length,
										lines: others.join(", "),
									})}>{m.viewers_devops_env_view_duplicate()}</span
								>
							{/if}
							{#if looksSecret(e.key)}<span class="rv-chip">{m.viewers_devops_env_view_secret()}</span>{/if}
							<button
								class="rv-btn icon"
								title={m.viewers_devops_go_to_line({ line: e.line })}
								aria-label={m.viewers_devops_go_to_line({ line: e.line })}
								onclick={() => ctx.toSource(e.line)}
							>
								<span class="ln">{e.line}</span>
							</button>
							<button
								class="rv-btn icon"
								title={shown(e)
									? m.viewers_devops_env_view_hide_value()
									: m.viewers_devops_env_view_reveal_value()}
								aria-label={shown(e)
									? m.viewers_devops_env_view_hide_value()
									: m.viewers_devops_env_view_reveal_value()}
								onclick={() => (revealed[e.line] = !revealed[e.line])}
							>
								{#if shown(e)}<EyeOff size={12} />{:else}<Eye size={12} />{/if}
							</button>
							<Copy text={e.key} label={m.viewers_devops_env_view_copy_key()} key />
							<Copy text={e.value} label={m.viewers_devops_env_view_copy_value()} />
						</div>
					{:else if e.kind === "comment"}
						<div class="rv-row cm"><span class="rv-note">{e.text}</span></div>
					{:else}
						<div class="rv-row bad">
							<TriangleAlert size={13} />
							<span class="rv-v">{e.raw}</span>
							<span class="rv-note">{e.reason}</span>
							<button class="rv-btn" onclick={() => ctx.toSource(e.line)}
								>{m.viewers_devops_line({ line: e.line })}</button
							>
						</div>
					{/if}
				{:else}
					<div class="rv-empty">
						{q ? m.viewers_devops_env_view_no_match() : m.viewers_devops_env_view_none_yet()}
					</div>
				{/each}
			</div>
		</section>
	</div>
</div>

<style>
	.masked {
		letter-spacing: 0.08em;
		color: var(--ca-text-disabled);
	}
	.empty {
		font-style: italic;
		color: var(--ca-text-secondary);
		font-family: var(--ui-font, inherit);
	}
	.cm {
		min-height: 24px;
		background: transparent;
	}
	.ln {
		font-family: var(--mono);
		font-size: 10px;
		color: var(--ca-text-disabled);
	}
</style>
