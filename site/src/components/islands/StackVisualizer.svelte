<script lang="ts">
	import { YAML } from "../../data/stack";
	// Parses a tiny subset of atlas.yaml (services + depends_on + port) client-side and draws the start order.
	// Not a YAML parser: it understands the shape of the example and of edits to it.
	const initial = YAML;

	let src = $state(initial);

	type Svc = { name: string; port?: string; deps: { on: string; cond: string }[]; oneshot: boolean };

	function parse(text: string): { services: Svc[]; infra: boolean; error: string } {
		const lines = text.split("\n");
		const services: Svc[] = [];
		let section = "";
		let cur: Svc | null = null;
		let infra = false;
		for (const raw of lines) {
			if (!raw.trim() || raw.trim().startsWith("#")) continue;
			const indent = raw.length - raw.trimStart().length;
			const line = raw.trim();
			if (indent === 0) {
				section = line.replace(/:.*/, "");
				if (section === "infra") infra = true;
				cur = null;
				continue;
			}
			if (section !== "services") continue;
			if (indent === 2 && line.endsWith(":")) {
				cur = { name: line.slice(0, -1), deps: [], oneshot: false };
				services.push(cur);
			} else if (cur && indent >= 4) {
				const [k, ...rest] = line.split(":");
				const v = rest.join(":").trim();
				if (k === "port") cur.port = v;
				if (k === "oneshot") cur.oneshot = v === "true";
				if (k === "depends_on") {
					const m = v.match(/^\{(.*)\}$/);
					if (m) {
						cur.deps = m[1]
							.split(",")
							.map((p) => p.split(":").map((s) => s.trim()))
							.filter((p) => p[0] && p[1])
							.map(([on, cond]) => ({ on, cond }));
					}
				}
			}
		}
		const names = new Set(services.map((s) => s.name));
		for (const s of services) {
			for (const d of s.deps) {
				if (!names.has(d.on)) return { services, infra, error: `${s.name} depends on unknown service "${d.on}"` };
			}
		}
		return { services, infra, error: "" };
	}

	const parsed = $derived(parse(src));

	// Topological layers: layer = 1 + max(layer of deps).
	const layers = $derived.by(() => {
		const { services, error } = parsed;
		if (error) return [] as Svc[][];
		const level = new Map<string, number>();
		const visit = (s: Svc, seen: Set<string>): number => {
			if (level.has(s.name)) return level.get(s.name)!;
			if (seen.has(s.name)) return 0;
			seen.add(s.name);
			const l = s.deps.length
				? 1 + Math.max(...s.deps.map((d) => visit(services.find((x) => x.name === d.on)!, seen)))
				: 0;
			level.set(s.name, l);
			return l;
		};
		services.forEach((s) => visit(s, new Set()));
		const out: Svc[][] = [];
		for (const s of services) (out[level.get(s.name)!] ??= []).push(s);
		return out.filter(Boolean);
	});
</script>

<div class="viz">
	<div class="sheet">
		<div class="bar"><span>atlas.yaml</span><span class="hint">Edit me. Change a depends_on.</span></div>
		<textarea bind:value={src} spellcheck="false" autocapitalize="none" autocorrect="off" wrap="off" aria-label="atlas.yaml example. Edit it to see the start order change."></textarea>
	</div>
	<div class="out">
		<p class="cap">Start order, <span aria-live="polite">{parsed.error ? "blocked" : `${layers.length} ${layers.length === 1 ? "wave" : "waves"}`}</span></p>
		<div class="graph" aria-live="polite">
			{#if parsed.error}
				<p class="err"><i class="dot fail"></i> {parsed.error}. Nothing starts.</p>
			{:else}
				{#if parsed.infra}
					<div class="wave">
						<div class="node infra"><b>infra</b><small>docker compose up, waited on</small></div>
					</div>
				{/if}
				{#each layers as layer}
					<div class="wave">
						{#each layer as s (s.name)}
							<div class="node" class:one={s.oneshot}>
								<b>{s.name}</b>
								<small>{s.port ? `:${s.port}` : s.oneshot ? "runs once, must exit 0" : "no port"}</small>
								{#each s.deps as d}<span class="dep">after {d.on} is {d.cond.replace(/_/g, " ")}</span>{/each}
							</div>
						{/each}
					</div>
				{/each}
			{/if}
		</div>
	</div>
</div>

<style>
	.viz { display: grid; grid-template-columns: minmax(0, 1.05fr) minmax(0, 1fr); gap: clamp(1.2rem, 3vw, 3rem); align-items: start; }
	.sheet { background: #fff; border: 2px solid var(--ink); box-shadow: 8px 8px 0 var(--ink); border-radius: 3px; }
	.bar { display: flex; justify-content: space-between; gap: 1rem; padding: 0.6rem 1rem; border-bottom: 2px solid var(--ink); font: 700 0.9rem/1.2 var(--font); font-stretch: 85%; }
	.hint { font-weight: 500; color: var(--ink-2); }
	textarea { display: block; width: 100%; min-height: 25rem; resize: vertical; padding: 1rem; border: 0; background: transparent; color: var(--ink); font: 500 0.84rem/1.6 var(--mono); tab-size: 2; }
	textarea:focus { outline: 3px solid var(--blue); outline-offset: -3px; }
	.cap { margin: 0 0 1rem; font-weight: 800; font-stretch: 75%; font-size: 1.5rem; }
	.graph { display: grid; gap: 0; }
	.wave { display: flex; flex-wrap: wrap; gap: 0.7rem; padding: 0 0 0 1.4rem; border-left: 3px solid var(--ink); position: relative; padding-bottom: 1rem; }
	.wave::before { content: ""; position: absolute; left: -9px; top: 0.9rem; width: 15px; height: 15px; background: var(--paper); border: 3px solid var(--ink); border-radius: 50%; }
	.wave:last-child { border-left-color: transparent; padding-bottom: 0; }
	.node { background: var(--blue); color: #fff; padding: 0.55rem 0.8rem; border-radius: 3px; min-width: 8.5rem; display: grid; gap: 0.05rem; }
	.node b { font-weight: 800; font-stretch: 80%; font-size: 1.05rem; }
	.node small { font: 500 0.74rem/1.3 var(--mono); color: var(--port); }
	.node .dep { font-size: 0.78rem; opacity: 0.85; margin-top: 0.15rem; }
	.node.infra { background: var(--ink); }
	.node.one { background: var(--blue-deep); }
	.err { display: flex; gap: 0.6rem; align-items: baseline; margin: 0; padding: 0.9rem 1rem; border: 2px solid var(--ink); background: #fff; font-weight: 600; }
	.err .dot { background: #d6281a; }
	@media (max-width: 860px) {
		.viz { grid-template-columns: 1fr; }
		.sheet { box-shadow: 5px 5px 0 var(--ink); }
		.bar { flex-wrap: wrap; gap: 0.2rem 1rem; font-size: 1rem; }
		.hint { font-size: 0.9rem; }
		.node small { font-size: 0.8125rem; }
		.node .dep { font-size: 0.8125rem; }
		.node { min-width: 9.5rem; }
	}
	/* 16px+ in editable fields: below that iOS Safari zooms the page on focus */
	@media (pointer: coarse) { textarea { font-size: 1rem; line-height: 1.55; overflow: auto; } }
</style>
