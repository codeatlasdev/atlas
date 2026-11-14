/**
 * Engine-step vocabulary of the Atlas Crew onboarding and the per-step copy: the phase an engine is in on the crew
 * server, its card label/badge, how a `crew_agent_check` answer maps to a phase, and the headline/body text of each
 * step. Pure data and functions: no Svelte, no Tauri, so it is bun-testable.
 */
import type { OnboardingStep } from "#lib/crew/onboardingFlow.ts";

export type EnginePhase =
	"idle" | "checking" | "not_installed" | "installing" | "install_error" | "needs_signin" | "ready";

export interface EngineStatus {
	phase: EnginePhase;
	error?: string;
	log?: string;
}

/** What `crew_agent_check` reports about one engine on the crew server. */
export interface AgentCheck {
	installed: boolean;
	signed_in: boolean;
}

/** Label of the status badge on an engine card. */
export function engineStatusLabel(phase: EnginePhase): string {
	switch (phase) {
		case "idle":
		case "checking":
			return "Checking…";
		case "not_installed":
			return "Install";
		case "installing":
			return "Installing…";
		case "install_error":
			return "Retry";
		case "needs_signin":
			return "Sign in";
		case "ready":
			return "Ready";
	}
}

/** Whether the engine is busy (checking or installing): the card shows a spinner and ignores clicks. */
export function engineBusy(phase: EnginePhase): boolean {
	return phase === "installing" || phase === "checking" || phase === "idle";
}

/** The CSS modifier of an engine card's status badge. */
export function engineBadge(phase: EnginePhase): "ready" | "action" | "busy" | "error" {
	if (phase === "ready") return "ready";
	if (phase === "not_installed" || phase === "needs_signin") return "action";
	if (phase === "install_error") return "error";
	return "busy";
}

/** The phase an engine is in once `crew_agent_check` answered (outside an install: a missing binary is "not installed"). */
export function phaseFromCheck(r: AgentCheck): EnginePhase {
	return r.installed ? (r.signed_in ? "ready" : "needs_signin") : "not_installed";
}

/** The status after an install finished and the engine was checked again: a missing binary is an install error. */
export function statusAfterInstall(r: AgentCheck): EngineStatus {
	return {
		phase: r.installed ? (r.signed_in ? "ready" : "needs_signin") : "install_error",
		error: r.installed ? undefined : "Install finished but binary not found",
	};
}

/**
 * The engine to continue with: the one picked if it's Ready, else the first Ready one on the crew server
 * (`engines` in display order). "" when none is Ready.
 */
export function pickReadyEngine(
	selected: string,
	engines: { id: string }[],
	status: Record<string, EngineStatus>,
): string {
	if (selected !== "" && status[selected]?.phase === "ready") return selected;
	return engines.find((a) => status[a.id]?.phase === "ready")?.id ?? "";
}

/** One line of the how-it-works story, lit in sync with the art. */
export const STORY = [
	{ title: "You give the order", text: "Type what you want, like a message." },
	{ title: "It works on your server", text: "All night, even with your Mac asleep." },
	{ title: "It asks before anything risky", text: "Allow or deny from your Mac, phone or Telegram." },
	{ title: "You wake up to results", text: "Every step is logged, ready to read." },
];

export type StepMeta = {
	label?: string;
	headline: string;
	body: string;
	bullets?: string[];
	glow: string;
};

export const STEP_META: Record<OnboardingStep, StepMeta> = {
	intro: {
		label: "1 of 2",
		headline: "Meet your Atlas Crew",
		body: "Your crew sails while you sleep.",
		bullets: [
			"Your server, your rules — runs on your server, no account",
			"Hand it the wheel — it keeps working while your Mac sleeps",
			"It flags you before anything risky",
		],
		glow: "rgba(61, 91, 255, 0.10)",
	},
	how: {
		label: "2 of 2",
		headline: "One night with your crew",
		body: "This is what a crewmate does while you're away.",
		bullets: undefined,
		glow: "rgba(61, 91, 255, 0.10)",
	},
	connect: {
		headline: "Add a crew server",
		body: "You need a Linux server you can SSH into.",
		glow: "rgba(61, 91, 255, 0.08)",
	},
	engine: {
		headline: "Choose an engine",
		body: "Select the AI your crewmate will use. Each engine runs on your own subscription or API key.",
		glow: "rgba(61, 91, 255, 0.10)",
	},
	ready: {
		headline: "Your first crewmate is ready",
		body: "Open a chat to send your first task. You can add more crewmates and customise them from the workspace.",
		glow: "rgba(34, 197, 94, 0.12)",
	},
};
