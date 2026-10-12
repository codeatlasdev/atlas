<script lang="ts">
	import { onMount } from "svelte";

	type Line = { t: string; cls?: string; delay?: number };
	const script: Line[] = [
		{ t: "$ atlas dev", cls: "cmd", delay: 500 },
		{ t: "orbit-shop  5 services + infra", cls: "dim", delay: 300 },
		{ t: "infra     database           :54320   up", cls: "ok", delay: 500 },
		{ t: "migrate   bun run db:migrate", cls: "run", delay: 400 },
		{ t: "migrate   applied 001_init", cls: "dim", delay: 500 },
		{ t: "migrate   completed_successfully          1.2s", cls: "ok", delay: 450 },
		{ t: "api       bun run dev:api    :4100", cls: "run", delay: 350 },
		{ t: "api       waiting for http://localhost:4100/health", cls: "dim", delay: 700 },
		{ t: "api       healthy                          2.8s", cls: "ok", delay: 450 },
		{ t: "web       bun run dev:web    :4101", cls: "run", delay: 350 },
		{ t: "worker    bun run dev:worker", cls: "run", delay: 500 },
		{ t: "web       healthy                          1.1s", cls: "ok", delay: 450 },
		{ t: "worker    exited 1, restart in 1s (1/5)", cls: "warn", delay: 900 },
		{ t: "worker    running                          0.4s", cls: "ok", delay: 500 },
		{ t: "", delay: 200 },
		{ t: "5 of 5 up. logs in .logs/. ? for keys", cls: "fin", delay: 0 },
	];

	let shown = $state<Line[]>(script);
	let root: HTMLDivElement;
	let timer: ReturnType<typeof setTimeout> | undefined;
	let started = false;
	let reduce = false;

	function play(i = 0) {
		if (i >= script.length) {
			timer = setTimeout(() => {
				shown = [];
				play(0);
			}, 5000);
			return;
		}
		shown = [...shown, script[i]];
		timer = setTimeout(() => play(i + 1), script[i].delay ?? 300);
	}

	onMount(() => {
		reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;
		if (reduce) {
			shown = script;
			return;
		}
		shown = [];
		const io = new IntersectionObserver(
			([e]) => {
				if (e.isIntersecting && !started) {
					started = true;
					play();
				} else if (!e.isIntersecting && started) {
					clearTimeout(timer);
					started = false;
					shown = [];
				}
			},
			{ threshold: 0.3 },
		);
		io.observe(root);
		return () => {
			io.disconnect();
			clearTimeout(timer);
		};
	});
</script>

<div class="term" bind:this={root} role="img" aria-label="Replay of atlas dev: infra starts, migrations run, the api becomes healthy, web and worker start, and a crashing worker restarts with backoff.">
	<div class="chrome">
		<i></i><i></i><i></i><span>orbit-shop: atlas dev</span>
	</div>
	<pre aria-hidden="true">{#each shown as l}<span class={l.cls}>{l.t}
</span>{/each}<span class="caret">▍</span></pre>
</div>

<style>
	.term { background: #0a0b3d; border-radius: 10px; overflow: hidden; box-shadow: 0 0 0 1px rgb(12 16 56 / 0.5), 8px 8px 0 var(--blue-deep); text-align: left; color: #fff; }
	.chrome { display: flex; align-items: center; gap: 7px; padding: 10px 14px; border-bottom: 1px solid rgb(255 255 255 / 0.16); }
	.chrome i { width: 11px; height: 11px; border-radius: 50%; background: rgb(255 255 255 / 0.28); }
	.chrome span { margin-left: 10px; font: 600 0.82rem var(--font); color: rgb(255 255 255 / 0.75); }
	pre { margin: 0; padding: 18px 20px; min-height: 24.5em; font-size: 0.82rem; line-height: 1.6; color: #dfe4ff; overflow-x: auto; }
	.cmd { color: #fff; font-weight: 700; } .dim { color: rgb(255 255 255 / 0.55); } .ok { color: var(--go); } .run { color: var(--port); }
	.warn { color: var(--wait); } .fin { color: #fff; font-weight: 700; }
	.caret { color: #fff; animation: blink 1s steps(2) infinite; }
	@keyframes blink { 50% { opacity: 0; } }
	@media (max-width: 560px) { pre { font-size: 0.8125rem; padding: 14px; min-height: calc(17 * 1.6em + 28px); } .term { box-shadow: 0 0 0 1px rgb(12 16 56 / 0.5), 5px 5px 0 var(--blue-deep); } }
</style>
