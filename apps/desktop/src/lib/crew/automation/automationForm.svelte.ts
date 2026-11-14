/**
 * Form state of the add / edit automation dialog: the fields of every trigger kind, loading them from an existing
 * trigger, and building the wire `Trigger` back. The dialog and its field components only bind to this.
 */
import type { Trigger, TriggerKind } from "#lib/crew.ts";
import {
	cronFromSchedule,
	describeTrigger,
	intervalFromSecs,
	intervalToSecs,
	scheduleFromCron,
	type IntervalUnit,
} from "#lib/crew/automations.ts";

const WEEKDAYS = [1, 2, 3, 4, 5];

export class AutomationForm {
	kind = $state<TriggerKind>("schedule");
	prompt = $state("");

	scheduleDays = $state<number[]>([...WEEKDAYS]);
	scheduleHour = $state(9);
	scheduleMinute = $state(0);
	customCronMode = $state(false);
	customCron = $state("");

	intervalValue = $state(30);
	intervalUnit = $state<IntervalUnit>("minutes");

	githubRepo = $state("");
	githubEvents = $state<Set<string>>(new Set());

	filePath = $state("");
	fileGlob = $state("");

	crewmateChainFrom = $state("");
	crewmateChainOn = $state<"done" | "failed">("done");

	serviceProjectDir = $state("");
	serviceName = $state("");
	serviceOn = $state<"failed" | "recovered">("failed");

	webhookToken = $state("");

	/** Cron of the schedule kind: the typed one in custom mode, else built from days + time. */
	readonly scheduleCron = $derived(
		this.customCronMode
			? this.customCron
			: cronFromSchedule({ days: this.scheduleDays, hour: this.scheduleHour, minute: this.scheduleMinute }),
	);

	/** The schedule in words ("Every weekday at 9:00 AM"), shown live under the builder. */
	liveSentence(tz?: string): string {
		return this.kind === "schedule" && this.scheduleCron
			? describeTrigger({ type: "schedule", cron: this.scheduleCron }, tz)
			: "";
	}

	/** True when the fields of the chosen kind (plus the prompt) are enough to save. */
	get valid(): boolean {
		if (!this.prompt.trim()) return false;
		switch (this.kind) {
			case "schedule":
				return !!this.scheduleCron;
			case "interval":
				return this.intervalValue >= 1;
			case "github":
				return !!this.githubRepo.trim();
			case "file":
				return !!(this.filePath.trim() || this.fileGlob.trim());
			case "crewmate":
				return !!this.crewmateChainFrom.trim();
			case "service":
				return !!this.serviceName.trim();
			case "webhook":
				return true;
		}
	}

	toggleDay(d: number) {
		this.scheduleDays = this.scheduleDays.includes(d)
			? this.scheduleDays.length > 1
				? this.scheduleDays.filter((x) => x !== d)
				: this.scheduleDays
			: [...this.scheduleDays, d];
	}

	toggleGithubEvent(ev: string, on: boolean) {
		const s = new Set(this.githubEvents);
		if (on) s.add(ev);
		else s.delete(ev);
		this.githubEvents = s;
	}

	/** Blank form (a new automation), or the fields of `trigger` when editing. */
	load(trigger: Trigger | null) {
		this.reset();
		if (!trigger) return;
		this.kind = trigger.kind.type as TriggerKind;
		this.prompt = trigger.prompt;
		const k = trigger.kind;
		if (k.type === "schedule") {
			const spec = scheduleFromCron(k.cron);
			this.customCron = k.cron;
			if (spec) {
				this.scheduleDays = spec.days;
				this.scheduleHour = spec.hour;
				this.scheduleMinute = spec.minute;
			} else {
				this.customCronMode = true;
			}
		} else if (k.type === "interval") {
			const { value, unit } = intervalFromSecs(k.every_secs);
			this.intervalValue = value;
			this.intervalUnit = unit;
		} else if (k.type === "github") {
			this.githubRepo = k.repo;
			this.githubEvents = new Set(k.events);
		} else if (k.type === "file") {
			this.filePath = k.path;
			this.fileGlob = k.glob ?? "";
		} else if (k.type === "crewmate") {
			this.crewmateChainFrom = k.from_crewmate;
			this.crewmateChainOn = k.on;
		} else if (k.type === "service") {
			this.serviceProjectDir = k.project_dir;
			this.serviceName = k.service;
			this.serviceOn = k.on;
		} else if (k.type === "webhook") {
			this.webhookToken = k.token;
		}
	}

	private reset() {
		this.kind = "schedule";
		this.prompt = "";
		this.scheduleDays = [...WEEKDAYS];
		this.scheduleHour = 9;
		this.scheduleMinute = 0;
		this.customCronMode = false;
		this.customCron = "";
		this.intervalValue = 30;
		this.intervalUnit = "minutes";
		this.githubRepo = "";
		this.githubEvents = new Set();
		this.filePath = "";
		this.fileGlob = "";
		this.crewmateChainFrom = "";
		this.crewmateChainOn = "done";
		this.serviceProjectDir = "";
		this.serviceName = "";
		this.serviceOn = "failed";
		this.webhookToken = "";
	}

	/** The wire trigger kind for the chosen kind (a webhook without a token gets a fresh one). */
	toKind(): Trigger["kind"] {
		switch (this.kind) {
			case "schedule":
				return { type: "schedule", cron: this.scheduleCron };
			case "interval":
				return { type: "interval", every_secs: intervalToSecs(this.intervalValue, this.intervalUnit) };
			case "github":
				return { type: "github", repo: this.githubRepo.trim(), events: [...this.githubEvents] };
			case "file":
				return { type: "file", path: this.filePath.trim(), glob: this.fileGlob.trim() || undefined };
			case "crewmate":
				return { type: "crewmate", from_crewmate: this.crewmateChainFrom.trim(), on: this.crewmateChainOn };
			case "service":
				return {
					type: "service",
					project_dir: this.serviceProjectDir.trim(),
					service: this.serviceName.trim(),
					on: this.serviceOn,
				};
			case "webhook":
				return {
					type: "webhook",
					token: this.webhookToken || crypto.randomUUID().replace(/-/g, "").slice(0, 24),
				};
		}
	}
}
