import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { toast } from "@codeatlasdev/ui";
import { api, errText, type DiscoverDone } from "#lib/api.ts";
import { addable, parsePrefs, type Found } from "#lib/components/sidebar/discovery.ts";
import { sound } from "#lib/sound/sound.svelte.ts";
import { discoverySound } from "#lib/sound/triggers.ts";
import { store } from "#lib/stores/projects.svelte.ts";

type ScanEvent =
	| ({ t: "found" } & Found)
	| { t: "progress"; dirs: number; found: number; elapsed_ms: number; current: string };

const PREFS_KEY = "atlas.discover";

/** The "Find projects" scan: results stream in while the Rust walk runs off the main thread. */
class Discovery {
	open = $state(false);
	running = $state(false);
	found = $state<Found[]>([]);
	dirs = $state(0);
	elapsedMs = $state(0);
	current = $state("");
	done = $state<DiscoverDone | null>(null);
	error = $state<string | null>(null);
	#saved = parsePrefs(localStorage.getItem(PREFS_KEY));
	/** User folders scanned in addition to the defaults. */
	roots = $state<string[]>(this.#saved.roots);
	candidates = $state(this.#saved.candidates);

	#unlisten: UnlistenFn | null = null;
	#run = 0;

	/** Folders already in the sidebar (matched by path). */
	registered = $derived(new Set(store.projects.map((p) => p.root)));
	addable = $derived(addable(this.found, this.registered));

	#save() {
		localStorage.setItem(PREFS_KEY, JSON.stringify({ roots: this.roots, candidates: this.candidates }));
	}

	setCandidates(on: boolean) {
		this.candidates = on;
		this.#save();
	}

	addRoot(dir: string) {
		if (this.roots.includes(dir)) return;
		this.roots = [...this.roots, dir];
		this.#save();
	}

	removeRoot(dir: string) {
		this.roots = this.roots.filter((r) => r !== dir);
		this.#save();
	}

	/** Open the dialog and start a fresh scan. */
	async show() {
		this.open = true;
		await this.start();
	}

	async start() {
		const run = ++this.#run;
		this.#unlisten?.();
		this.found = [];
		this.dirs = 0;
		this.elapsedMs = 0;
		this.current = "";
		this.done = null;
		this.error = null;
		this.running = true;
		this.#unlisten = await listen<ScanEvent>("discover://event", (e) => this.#apply(e.payload));
		try {
			const done = await api.discover({ roots: this.roots, candidates: this.candidates });
			if (run === this.#run) {
				this.done = done;
				const cue = discoverySound(done, this.addable.length, null);
				if (cue) sound.play(cue);
			}
		} catch (e) {
			if (run === this.#run) this.error = errText(e);
		} finally {
			if (run === this.#run) {
				this.running = false;
				this.#unlisten?.();
				this.#unlisten = null;
			}
		}
	}

	#apply(ev: ScanEvent) {
		if (ev.t === "found") {
			const { t: _, ...f } = ev;
			const i = this.found.findIndex((x) => x.path === f.path);
			if (i < 0) this.found.push(f);
			else this.found[i] = f;
		} else {
			this.dirs = ev.dirs;
			this.elapsedMs = ev.elapsed_ms;
			this.current = ev.current;
		}
	}

	async cancel() {
		await api.discoverCancel().catch(() => {});
	}

	/** Stop the scan and close the dialog. */
	async close() {
		this.open = false;
		if (this.running) await this.cancel();
	}

	/** Candidate → `atlas init` + add. Resolves with the new project's id. */
	async init(dir: string): Promise<string | undefined> {
		const p = await store.run(`init:${dir}`, () => api.initProject(dir));
		if (!p) return;
		toast.success(`Created atlas.yaml in ${p.name}`);
		await store.refresh();
		// The folder now has a config: show it as a regular project.
		const i = this.found.findIndex((x) => x.path === dir);
		if (i >= 0)
			this.found[i] = { ...this.found[i], has_config: true, name: p.name, services: p.services_total };
		return p.id;
	}
}

export const discovery = new Discovery();
