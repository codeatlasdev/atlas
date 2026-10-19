<script lang="ts">
	import { kbd } from "#lib/keys.ts";
	import { Button, Alert, Dialog, Spinner, toast, Kbd } from "@codeatlasdev/ui";
	import { Save, RotateCcw, CircleCheck, CircleAlert } from "@lucide/svelte";
	import { EditorView, basicSetup } from "codemirror";
	import { EditorState } from "@codemirror/state";
	import { keymap } from "@codemirror/view";
	import { atlasTheme } from "#lib/cm.ts";
	import { savers } from "#lib/symbols.ts";
	import { yamlSchema } from "codemirror-json-schema/yaml";
	import { highlightLines } from "#lib/viewers/rich/highlight.ts";
	import { onDestroy, onMount } from "svelte";
	import { api, errText, type Problem, type ProjectView } from "#lib/api.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { diff } from "#lib/diff.ts";
	import { tilde } from "#lib/format.ts";

	let { project }: { project: ProjectView } = $props();

	let host: HTMLDivElement | undefined = $state();
	let view: EditorView | undefined;
	let disk = $state("");
	let text = $state("");
	let path = $state("");
	let problems = $state<Problem[]>([]);
	let validating = $state(false);
	let saving = $state(false);
	let diffOpen = $state(false);
	let diffRows = $state<{ kind: "add" | "del" | "ctx"; html: string }[]>([]);
	let externalChange = $state(false);
	let savedOnce = $state(false);

	let dirty = $derived(text !== disk);
	let configChanged = $derived(store.live[project.id]?.configChanged ?? 0);

	function setDoc(t: string) {
		view?.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: t } });
		text = t;
	}

	async function load() {
		const r = await store.run("cfg", () => api.configRead(project.id));
		if (!r) return;
		disk = r.text;
		path = r.path;
		setDoc(r.text);
		externalChange = false;
	}

	onMount(async () => {
		const schema = JSON.parse(await api.configSchema());
		const r = await api.configRead(project.id).catch((e) => {
			toast.error(errText(e));
			return { text: "", path: "" };
		});
		disk = r.text;
		text = r.text;
		path = r.path;
		view = new EditorView({
			parent: host,
			state: EditorState.create({
				doc: r.text,
				extensions: [
					basicSetup,
					yamlSchema(schema),
					keymap.of([{ key: "Mod-s", preventDefault: true, run: () => (review(), true) }]),
					EditorView.updateListener.of((u) => {
						if (u.docChanged) text = u.state.doc.toString();
					}),
					atlasTheme,
				],
			}),
		});
	});
	/** ⌘S (menu / keymap `workspace::Save`): the same review-then-save as this editor's Mod-s. */
	const saverKey = $derived(`config:${project.id}`);
	$effect(() => {
		const key = saverKey;
		savers.set(key, review);
		return () => void savers.delete(key);
	});
	onDestroy(() => {
		clearTimeout(timer);
		view?.destroy();
	});

	// Live validation through the same loader the CLI uses (300ms debounce).
	let timer: ReturnType<typeof setTimeout> | undefined;
	$effect(() => {
		const t = text;
		if (!path) return;
		clearTimeout(timer);
		validating = true;
		timer = setTimeout(async () => {
			try {
				problems = await api.configValidate(project.id, t);
			} catch (e) {
				problems = [{ path: "", message: errText(e) }];
			}
			validating = false;
		}, 300);
	});

	// atlas.yaml changed on disk.
	let seenChange = 0;
	$effect(() => {
		const n = configChanged;
		if (n === seenChange) return;
		seenChange = n;
		if (saving) return;
		api.configRead(project.id).then((r) => {
			if (r.text === disk) return;
			if (dirty) externalChange = true;
			else {
				disk = r.text;
				setDoc(r.text);
			}
		});
	});

	function jump(p: Problem) {
		if (!view || !p.path) return;
		// Find the last path segment as a YAML key and select it.
		const key = p.path.split(".").pop()!;
		const m =
			new RegExp(`^\\s*-?\\s*${key.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\s*:`, "m").exec(text) ??
			new RegExp(`\\b${key.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\b`).exec(text);
		if (!m) return;
		const from = m.index + m[0].indexOf(key);
		view.dispatch({ selection: { anchor: from, head: from + key.length }, scrollIntoView: true });
		view.focus();
	}

	async function review() {
		if (!dirty || problems.length || validating) return;
		const rows = diff(disk, text).split("\n");
		const html = await highlightLines(rows.map((r) => r.slice(2)).join("\n"), "yaml");
		diffRows = rows.map((r, i) => ({
			kind: r[0] === "+" ? "add" : r[0] === "-" ? "del" : "ctx",
			html: html[i],
		}));
		diffOpen = true;
	}

	async function save() {
		diffOpen = false;
		saving = true;
		const ok = await store.run("save", () => api.configWrite(project.id, text).then(() => true));
		saving = false;
		if (!ok) return;
		disk = text;
		externalChange = false;
		savedOnce = true;
		toast.success("atlas.yaml saved");
		store.loadConfig(project.id);
	}

	async function restart() {
		savedOnce = false;
		const id = project.id;
		await store.run(`stack:${id}`, async () => {
			await api.stop(id);
			await api.start(id);
		});
	}
</script>

<div class="bar">
	<span class="path mono">{tilde(path)}</span>
	{#if dirty}<span class="dirty">● edited</span>{/if}
	<div class="status">
		{#if validating}
			<Spinner size={13} /> validating
		{:else if problems.length}
			<CircleAlert size={14} class="bad" /> {problems.length} problem{problems.length === 1 ? "" : "s"}
		{:else}
			<CircleCheck size={14} class="good" /> valid
		{/if}
	</div>
	<Button variant="ghost" size="sm" disabled={!dirty} onclick={() => setDoc(disk)}>
		{#snippet leading()}<RotateCcw size={13} />{/snippet}
		Revert
	</Button>
	<Button size="sm" disabled={!dirty || problems.length > 0 || validating || saving} onclick={review}>
		{#snippet leading()}<Save size={13} />{/snippet}
		Save
		{#snippet trailing()}<Kbd>{kbd("cmd-s")}</Kbd>{/snippet}
	</Button>
</div>

{#if externalChange}
	<div class="banner">
		<Alert severity="warning" title="File changed on disk">
			<div class="row">
				Your edits are unsaved.
				<Button size="sm" variant="glass" onclick={load}>Reload</Button>
				<Button size="sm" variant="ghost" onclick={() => (externalChange = false)}>Keep mine</Button>
			</div>
		</Alert>
	</div>
{/if}
{#if savedOnce && project.running}
	<div class="banner">
		<Alert severity="info" closable onclose={() => (savedOnce = false)}>
			<div class="row">
				The running stack still uses the old config.
				<Button size="sm" variant="glass" disabled={store.busy[`stack:${project.id}`]} onclick={restart}
					>Restart stack to apply</Button
				>
			</div>
		</Alert>
	</div>
{/if}

<div class="split">
	<div class="editor ca-selectable" bind:this={host}></div>
	{#if problems.length}
		<aside class="problems">
			{#each problems as p, i (i)}
				<button class="problem" onclick={() => jump(p)}>
					{#if p.path}<span class="ppath mono">{p.path}</span>{/if}
					<span>{p.message}</span>
				</button>
			{/each}
		</aside>
	{/if}
</div>

<Dialog bind:open={diffOpen} title="Save atlas.yaml?" description="Review the changes." size="lg">
	<div class="diff ca-selectable">
		{#each diffRows as r, i (i)}
			<div class="row-{r.kind}">
				<span class="sign">{r.kind === "add" ? "+" : r.kind === "del" ? "-" : " "}</span>{@html r.html}
			</div>
		{/each}
	</div>
	{#snippet actions()}
		<Button variant="ghost" onclick={() => (diffOpen = false)}>Cancel</Button>
		<Button onclick={save}>Save</Button>
	{/snippet}
</Dialog>

<style>
	.bar {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 8px 16px;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
	}
	.path {
		font-size: 11.5px;
		color: var(--ca-text-secondary);
	}
	.dirty {
		font-size: 11.5px;
		color: var(--ca-semantic-warning);
	}
	.status {
		margin-left: auto;
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.status :global(.bad) {
		color: var(--ca-semantic-error);
	}
	.status :global(.good) {
		color: var(--ca-semantic-success);
	}
	.banner {
		padding: 8px 16px 0;
	}
	.row {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.split {
		flex: 1;
		min-height: 0;
		display: flex;
	}
	.editor {
		flex: 1;
		min-width: 0;
		overflow: hidden;
	}
	.editor :global(.cm-editor) {
		height: 100%;
	}
	.problems {
		width: 300px;
		overflow-y: auto;
		border-left: 0.5px solid var(--ca-outline-subtle);
		padding: 8px;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.problem {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 2px;
		text-align: left;
		border: 0;
		border-radius: 7px;
		padding: 8px 10px;
		background: color-mix(in oklab, var(--ca-semantic-error) 8%, transparent);
		color: var(--ca-text-primary);
		font: inherit;
		font-size: 12px;
		cursor: pointer;
	}
	.problem:hover {
		background: color-mix(in oklab, var(--ca-semantic-error) 15%, transparent);
	}
	.ppath {
		color: var(--ca-semantic-error);
		font-size: 11px;
	}
	.diff {
		max-height: 55vh;
		overflow: auto;
		border-radius: 8px;
		padding: 8px 0;
		font-family: var(--mono);
		font-size: 12px;
		background: var(--ca-surface-sunken);
	}
	.diff div {
		white-space: pre;
		padding: 0 12px;
	}
	.sign {
		display: inline-block;
		width: 2ch;
		user-select: none;
		color: var(--ca-text-disabled);
	}
	.row-add {
		background: color-mix(in oklab, var(--atlas-vcs-added, var(--ca-semantic-success)) 14%, transparent);
	}
	.row-add .sign {
		color: var(--atlas-vcs-added, var(--ca-semantic-success));
	}
	.row-del {
		background: color-mix(in oklab, var(--atlas-vcs-deleted, var(--ca-semantic-error)) 14%, transparent);
	}
	.row-del .sign {
		color: var(--atlas-vcs-deleted, var(--ca-semantic-error));
	}
</style>
