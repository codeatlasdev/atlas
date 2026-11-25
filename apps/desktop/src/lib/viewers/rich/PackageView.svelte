<script lang="ts">
	import { onMount } from "svelte";
	import { Play, ExternalLink, Package, Boxes, Search, TriangleAlert } from "@lucide/svelte";
	import { host } from "#lib/host.ts";
	import { api } from "#lib/api.ts";
	import { fileType } from "#lib/viewers/fileType.ts";
	import {
		detectPackageManager,
		manifestFor,
		npmManifest,
		runScriptArgv,
		type Manifest,
	} from "#lib/viewers/rich/manifest.ts";
	import { parseData } from "#lib/viewers/rich/data.ts";
	import { runInTerminal } from "#lib/viewers/rich/run.ts";
	import type { RichCtx } from "#lib/viewers/rich/ctx.ts";
	import Copy from "#lib/viewers/rich/Copy.svelte";
	import "#lib/viewers/rich/rich.css";
	import { m } from "#lib/i18n/m.ts";

	let { ctx }: { ctx: RichCtx } = $props();

	let id = $derived(fileType(ctx.path).id);
	let result = $derived.by((): { manifest: Manifest } | { error: string; line?: number } => {
		if (id === "package-json") {
			const p = parseData(ctx.text, "json");
			return p.ok ? { manifest: npmManifest(p.value) } : { error: p.error, line: p.line };
		}
		return manifestFor(id, ctx.text);
	});
	let pkg = $derived("manifest" in result ? result.manifest : null);
	let siblings = $state<string[]>([]);
	let q = $state("");
	let pm = $derived(detectPackageManager(siblings, pkg?.packageManager));

	onMount(() => {
		const dir = ctx.path.split("/").slice(0, -1).join("/");
		void host
			.fsList(`${ctx.root}/${dir}`)
			.then((r) => (siblings = r.map((e) => e.name)))
			.catch(() => {});
	});

	const needle = $derived(q.trim().toLowerCase());
	const match = (s: string) => !needle || s.toLowerCase().includes(needle);
	const url = (kind: Manifest["kind"], name: string, registry: string) =>
		kind === "go" ? `${registry}${name}` : `${registry}${encodeURIComponent(name).replace(/%2F/g, "/")}`;
	const pmLabel = $derived(
		id === "package-json" || id === "composer" ? (id === "composer" ? "composer" : pm) : "",
	);
	const argvFor = (script: string) =>
		id === "composer" ? ["composer", "run-script", script] : runScriptArgv(pm, script);
	const runnable = $derived(id === "package-json" || id === "composer");
</script>

<div class="rv">
	<div class="rv-inner">
		{#if "error" in result}
			<div class="rv-banner bad">
				<TriangleAlert size={14} />
				{m.viewers_devops_cannot_parse({ error: result.error })}
				<span class="sp"></span>
				<button class="rv-btn" onclick={() => ctx.toSource(result.line)}>
					{m.viewers_devops_open_as_source()}
				</button>
			</div>
		{:else if pkg}
			<header class="rv-head">
				<div class="rv-title">
					<Package size={18} />
					<h1>{pkg.name || ctx.path.split("/").pop()}</h1>
					{#if pkg.version}<span class="rv-ver">{pkg.version}</span>{/if}
					{#if pkg.license}<span class="rv-chip">{pkg.license}</span>{/if}
				</div>
				{#if pkg.description}<p class="rv-desc">{pkg.description}</p>{/if}
				<div class="rv-facts">
					{#each pkg.facts as f (f.label)}
						<span class="rv-chip"><b>{f.label}</b> {f.value}</span>
					{/each}
					{#each pkg.links as l (l.url)}
						<button class="rv-chip acc link" onclick={() => api.openUrl(l.url)}>
							<ExternalLink size={10} />
							{l.label}
						</button>
					{/each}
					{#if pkg.name && pkg.kind !== "go"}
						<button
							class="rv-chip acc link"
							onclick={() => api.openUrl(url(pkg.kind, pkg.name, pkg.registry))}
						>
							<ExternalLink size={10} />
							{m.viewers_files_package_view_registry()}
						</button>
					{/if}
				</div>
			</header>

			{#if pkg.scripts.length}
				<section class="rv-sec">
					<h2>
						{pkg.kind === "cargo"
							? m.viewers_files_package_view_features()
							: pkg.kind === "python"
								? m.viewers_files_package_view_entry_points()
								: m.viewers_files_package_view_scripts()}
						<span class="n">{pkg.scripts.length}</span>
						<span class="sp"></span>
						{#if runnable}<span class="rv-note">
								{m.viewers_files_package_view_runs_with({ pm: pmLabel })}
							</span>{/if}
					</h2>
					<div class="rv-list">
						{#each pkg.scripts as s (s.name)}
							<div class="rv-row">
								<span class="rv-k">{s.name}</span>
								<span class="rv-v" title={s.command}>{s.command}</span>
								{#if runnable}
									<button
										class="rv-btn run"
										title={m.viewers_files_package_view_run_in_terminal()}
										onclick={() => runInTerminal(ctx.worktreeId, ctx.path, argvFor(s.name), s.name)}
									>
										<Play size={11} />
										{m.viewers_devops_make_view_run()}
									</button>
								{/if}
								<Copy
									text={runnable ? argvFor(s.name).join(" ") : s.command}
									label={m.viewers_devops_copy_command()}
								/>
							</div>
						{/each}
					</div>
				</section>
			{/if}

			{#if pkg.workspaces.length}
				<section class="rv-sec">
					<h2>
						<Boxes size={13} />
						{m.viewers_files_package_view_workspaces()} <span class="n">{pkg.workspaces.length}</span>
					</h2>
					<div class="rv-facts">
						{#each pkg.workspaces as w (w)}<span class="rv-chip mono">{w}</span>{/each}
					</div>
				</section>
			{/if}

			{#each pkg.groups as g, gi (g.title)}
				{@const list = g.deps.filter((d) => match(d.name))}
				<section class="rv-sec">
					<h2>
						{g.title}
						<span class="n">{g.deps.length}</span>
						<span class="sp"></span>
						{#if gi === 0}
							<label class="rv-search">
								<Search size={12} />
								<input
									placeholder={m.viewers_files_package_view_filter_dependencies()}
									bind:value={q}
									aria-label={m.viewers_files_package_view_filter_dependencies()}
								/>
							</label>
						{/if}
					</h2>
					{#if list.length}
						<div class="rv-list">
							{#each list as d (d.name)}
								<div class="rv-row">
									<span class="rv-k">{d.name}</span>
									<span class="rv-chip mono">{d.range}</span>
									{#if d.note}<span class="rv-note">{d.note}</span>{/if}
									<span class="rv-v"></span>
									{#if pkg.kind !== "go" || d.name.includes(".")}
										<button
											class="rv-btn icon"
											title={m.viewers_files_package_view_open_on_the_registry()}
											aria-label={m.viewers_files_package_view_open_name_on_the_registry({ name: d.name })}
											onclick={() => api.openUrl(url(pkg.kind, d.name, pkg.registry))}
										>
											<ExternalLink size={12} />
										</button>
									{/if}
								</div>
							{/each}
						</div>
					{:else}
						<div class="rv-list"><div class="rv-empty">{m.viewers_devops_make_view_no_match()}</div></div>
					{/if}
				</section>
			{/each}
		{/if}
	</div>
</div>

<style>
	b {
		font-weight: 600;
		color: var(--ca-text-primary);
		margin-right: 2px;
	}
	.link {
		border: 0;
		cursor: pointer;
		font: inherit;
		font-size: 11px;
	}
	.link:hover {
		filter: brightness(1.15);
	}
</style>
