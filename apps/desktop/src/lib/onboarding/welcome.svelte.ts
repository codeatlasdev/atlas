/**
 * Glue between the pure first-run machine (flow.ts) and the app: runs the scan, adds what the user picks, starts
 * the first stack and watches it come up. No rules live here; they are in flow.ts and the sound policy.
 */
import { toast } from "@codeatlasdev/ui";
import { open as pickFolder } from "@tauri-apps/plugin-dialog";
import { api, errText } from "#lib/api.ts";
import { addPaths } from "#lib/components/sidebar/projects.ts";
import { discovery } from "#lib/stores/discovery.svelte.ts";
import { store } from "#lib/stores/projects.svelte.ts";
import { INITIAL, next, progress, summarize, type Event, type Phase } from "./flow.ts";

class Welcome {
	phase = $state<Phase>(INITIAL);
	/** Paths the user ticked in the results. */
	picked = $state<Record<string, boolean>>({});
	/** Bumped when a ripple should cross the sky (scan start, stack up). */
	ripple = $state(0);
	/** The project whose stack the flow started and is watching. */
	watching = $state<string | null>(null);
	/** The title sequence is over (played, skipped, or not needed): the copy may appear. */
	introDone = $state(false);
	/** Scene-clock time the stack came up, for the globe's completion hit; null until then. */
	upAt = $state<number | null>(null);

	/** Scene clock of the globe canvas (set by the Hero); the completion hit is stamped on it. */
	clock: () => number = () => 0;

	summary = $derived(summarize(discovery.found));
	/** Valid, not-yet-added projects, newest first: what the user can pick. */
	choices = $derived(
		[...discovery.addable]
			.filter((f) => !f.error && !f.nested)
			.sort((a, b) => (b.modified ?? 0) - (a.modified ?? 0)),
	);
	chosen = $derived(this.choices.filter((f) => this.picked[f.path] ?? true));
	started = $derived.by(() => {
		const live = this.watching ? store.live[this.watching] : undefined;
		// `mode` is set by the runner's snapshot (or its status file): it has reported.
		return progress(live?.services ?? [], live?.mode != null);
	});

	send(ev: Event) {
		const was = this.phase;
		this.phase = next(this.phase, ev);
		if (was !== this.phase && (this.phase === "scanning" || this.phase === "up")) this.ripple++;
		if (this.phase === "up" && was !== "up") this.upAt = this.clock();
	}

	/** The primary action: run the machine-wide scan with results streaming into the hero. */
	async scan() {
		this.send({ t: "scan" });
		await discovery.start();
		if (this.phase !== "scanning") return;
		if (discovery.error) this.send({ t: "scan-failed" });
		else if (discovery.done?.cancelled) this.send({ t: "cancel" });
		else this.send({ t: "scanned", projects: this.choices.length });
	}

	async cancelScan() {
		await discovery.cancel();
	}

	toggle(path: string) {
		this.picked[path] = !(this.picked[path] ?? true);
	}

	/** Add a single folder the user points at (secondary action). */
	async addFolder() {
		const dir = await pickFolder({ directory: true, title: "Add atlas project" });
		if (typeof dir === "string") await this.#add([dir]);
	}

	/** Run `atlas init` in a folder (stack detection), then add it. */
	async initFolder() {
		const dir = await pickFolder({ directory: true, title: "Set up a project with atlas init" });
		if (typeof dir !== "string") return;
		const p = await store.run("init", () => api.initProject(dir));
		if (p) {
			await store.refresh();
			await this.#started(p.id);
		}
	}

	/** Add the ticked projects and start the first one's stack. */
	async addChosen() {
		await this.#add(this.chosen.map((f) => f.path));
	}

	async #add(paths: string[]) {
		if (!paths.length) return;
		const before = new Set(store.projects.map((p) => p.id));
		try {
			await addPaths(paths);
		} catch (e) {
			toast.error(errText(e));
			return;
		}
		const fresh = store.projects.find((p) => !before.has(p.id)) ?? store.sorted[0];
		if (fresh) await this.#started(fresh.id);
	}

	async #started(id: string) {
		store.selected = id;
		this.watching = id;
		this.send({ t: "added" });
		// `store.run` toasts and returns undefined on failure; without this the user would sit in "starting".
		const ok = await store.run(`stack:${id}`, async () => (await api.start(id), true));
		if (!ok && this.phase === "starting") this.send({ t: "stack-failed" });
	}

	/** Re-evaluate once the watched stack reports: up, or failed with nothing starting. */
	observe() {
		const p = this.started;
		if (this.phase !== "starting") return;
		if (p.done) this.send({ t: "stack-up" });
		else if (p.failed > 0 && p.up + p.failed >= p.total) this.send({ t: "stack-failed" });
	}

	reset() {
		this.phase = INITIAL;
		this.picked = {};
		this.watching = null;
		this.upAt = null;
	}
}

export const welcome = new Welcome();
