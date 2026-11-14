/**
 * State and saves of the selected crewmate's settings dialog: role, approval, short replies, memory, usage and
 * the automation list edits. The shell resets it on every open; sections read the fields and call the saves.
 * Fields autosave (selects on change, text on blur / Cmd+Enter); the provider form is crew/providerForm.svelte.ts.
 */
import { toast } from "@codeatlasdev/ui";
import { errText } from "#lib/api.ts";
import type { Crewmate, Trigger } from "#lib/crew.ts";
import { crew } from "#lib/stores/crew.svelte.ts";
import { AutosaveField } from "#lib/crew/settings/autosaveField.svelte.ts";
import { isDirty, updateSavedRef } from "#lib/crew/settings/autosave.ts";
import { upsertTrigger, withTriggerEnabled, withoutTrigger } from "#lib/crew/settings/triggers.ts";
import { EMPTY_USAGE_TOTALS, usageBuckets, usageTotals } from "#lib/crew/settings/usage.ts";

export type ShortReplies = "crew" | "on" | "off";

/** What `crew.updateCrewmate` takes, plus `short_replies` (null = use the crew setting; the wire accepts it, the store type does not list it yet). */
type CrewmatePatch = Parameters<typeof crew.updateCrewmate>[2] & { short_replies?: boolean | null };

const shortRepliesOf = (v: boolean | undefined): ShortReplies =>
	v === true ? "on" : v === false ? "off" : "crew";

export class CrewmateSettings {
	role = $state("");
	roleSaved = $state("");
	readonly roleField = new AutosaveField();

	approval = $state<"ask" | "auto">("auto");
	readonly approvalField = new AutosaveField();

	shortReplies = $state<ShortReplies>("crew");
	readonly shortRepliesField = new AutosaveField();

	memory = $state("");
	memorySaved = $state("");
	memoryLoading = $state(false);
	readonly memoryField = new AutosaveField();

	usageData = $state<number[]>([]);
	usageTotals = $state({ ...EMPTY_USAGE_TOTALS });

	/** Fill every field from the crewmate and load memory + usage; called each time the dialog opens. */
	open(crewmate: Crewmate) {
		this.role = crewmate.role ?? "";
		this.roleSaved = crewmate.role ?? "";
		this.approval = crewmate.approval;
		this.shortReplies = shortRepliesOf(crewmate.short_replies);
		for (const f of [this.roleField, this.approvalField, this.shortRepliesField, this.memoryField]) f.reset();
		this.memory = "";
		this.memorySaved = "";
		this.usageData = [];
		this.usageTotals = { ...EMPTY_USAGE_TOTALS };
		void this.loadMemory();
		void this.loadUsage();
	}

	/** Update one crewmate field through the crew store. No-op without a selection. */
	private update(crewmate: Crewmate, patch: CrewmatePatch): Promise<void> {
		const hostKey = crew.selected?.hostKey;
		return hostKey ? crew.updateCrewmate(hostKey, crewmate.id, patch) : Promise.resolve();
	}

	async saveRole(crewmate: Crewmate) {
		if (!isDirty(this.role, this.roleSaved)) return;
		const attempted = this.role;
		const ok = await this.roleField.run(() => this.update(crewmate, { role: attempted }));
		this.roleSaved = updateSavedRef(this.roleSaved, attempted, ok);
	}

	saveApproval(crewmate: Crewmate) {
		return this.approvalField.run(() => this.update(crewmate, { approval: this.approval }));
	}

	saveShortReplies(crewmate: Crewmate) {
		const val = this.shortReplies === "on" ? true : this.shortReplies === "off" ? false : null;
		return this.shortRepliesField.run(() => this.update(crewmate, { short_replies: val }));
	}

	async loadMemory() {
		this.memoryLoading = true;
		try {
			const text = await crew.getMemory();
			this.memory = text;
			this.memorySaved = text;
		} catch {
			/* keep stale */
		}
		this.memoryLoading = false;
	}

	async saveMemory() {
		if (!isDirty(this.memory, this.memorySaved)) return;
		const attempted = this.memory;
		const ok = await this.memoryField.run(() => crew.setMemory(attempted));
		this.memorySaved = updateSavedRef(this.memorySaved, attempted, ok);
	}

	async loadUsage() {
		try {
			const { records } = await crew.getUsage();
			const now = Date.now();
			this.usageData = usageBuckets(records, now);
			this.usageTotals = usageTotals(records, now);
		} catch {
			/* keep stale */
		}
	}

	// ─── automations ────────────────────────────────────────────────────────

	/** Send the edited trigger list; resolves true on success, toasts and resolves false on failure. */
	private async setTriggers(list: Trigger[]): Promise<boolean> {
		try {
			await crew.setTriggers(list);
			return true;
		} catch (e) {
			toast.error(errText(e));
			return false;
		}
	}

	toggleTrigger(all: Trigger[], id: string, enabled: boolean) {
		return this.setTriggers(withTriggerEnabled(all, id, enabled));
	}

	saveTrigger(all: Trigger[], trig: Trigger) {
		return this.setTriggers(upsertTrigger(all, trig));
	}

	deleteTrigger(all: Trigger[], id: string) {
		return this.setTriggers(withoutTrigger(all, id));
	}
}
