/**
 * Onboarding state: first-run flag, tour and checklist, persisted under `onboarding`
 * in settings.json. Checklist items complete from real observed state (projects,
 * running stack, terminal/agent tabs) and from commands run through the keymap.
 */
import { api } from "#lib/api.ts";
import { recent } from "#lib/keymap.svelte.ts";
import { store } from "#lib/stores/projects.svelte.ts";
import { ws } from "#lib/stores/workspace.svelte.ts";
import {
	complete,
	isFirstRun,
	itemsFromRecent,
	parseState,
	EMPTY_STATE,
	type ChecklistId,
	type OnboardingState,
} from "./checklist.ts";

class Onboarding {
	state = $state<OnboardingState>(structuredClone(EMPTY_STATE));
	loaded = $state(false);
	tourOpen = $state(false);
	helpOpen = $state(false);
	/** No project yet and the welcome was never finished. */
	firstRun = $derived(isFirstRun(this.state, this.loaded && store.loaded, store.projects.length));

	async init() {
		this.state = parseState((await api.settings()).onboarding);
		this.loaded = true;
	}

	private async save() {
		await api.settingsPatch({ onboarding: $state.snapshot(this.state) });
	}

	/** Mark a checklist item done and persist, once. */
	async markDone(id: ChecklistId) {
		const next = complete(this.state, id);
		if (next === this.state) return;
		this.state = next;
		await this.save().catch(() => {});
	}

	/** First-run welcome finished or skipped. */
	async finishWelcome() {
		if (this.state.done) return;
		this.state = { ...this.state, done: true };
		await this.save().catch(() => {});
	}

	startTour() {
		this.tourOpen = true;
	}

	async endTour() {
		this.tourOpen = false;
		this.state = { ...this.state, done: true, toured: true };
		await this.save().catch(() => {});
	}

	async dismissChecklist(dismissed = true) {
		this.state = { ...this.state, checklist: { ...this.state.checklist, dismissed } };
		await this.save().catch(() => {});
	}

	/** Bring the checklist back (Help menu / settings) with progress kept. */
	showChecklist = () => this.dismissChecklist(false);

	/** Start over: welcome is skipped (projects exist), tour and checklist reset. */
	async restart() {
		this.state = { ...structuredClone(EMPTY_STATE), done: true };
		await this.save().catch(() => {});
		this.startTour();
	}

	/** Observe real state and commands; returns the stop function. Call once. */
	observe(): () => void {
		let prevRecent = [...recent.ids];
		return $effect.root(() => {
			$effect(() => {
				if (!this.loaded) return;
				if (store.projects.length) void this.markDone("project");
				if (store.projects.some((p) => p.running)) void this.markDone("stack");
				if (ws.doc.tabs.some((t) => t.kind === "terminal" && !t.agent)) void this.markDone("terminal");
				if (ws.doc.tabs.some((t) => !!t.agent)) void this.markDone("agent");
			});
			$effect(() => {
				const next = [...recent.ids];
				const ids = itemsFromRecent(prevRecent, next);
				prevRecent = next;
				if (this.loaded) for (const id of ids) void this.markDone(id);
			});
		});
	}
}

export const onboarding = new Onboarding();
