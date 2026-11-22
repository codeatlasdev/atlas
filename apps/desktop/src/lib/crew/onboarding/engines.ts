/**
 * Engine-step vocabulary of the Atlas Crew onboarding and the per-step copy: the phase an engine is in on the crew
 * server, its card label/badge, how a `crew_agent_check` answer maps to a phase, and the headline/body text of each
 * step. No Svelte and no Tauri, so it is bun-testable. The copy comes from the catalog and is read inside functions
 * (never at module load), so it follows a language switch.
 */
import type { OnboardingStep } from "#lib/crew/onboardingFlow.ts";
import { m } from "#lib/i18n/m.ts";

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
			return m.crew_onboarding_engine_status_checking();
		case "not_installed":
			return m.crew_onboarding_engine_status_install();
		case "installing":
			return m.crew_onboarding_engine_status_installing();
		case "install_error":
			return m.common_action_retry();
		case "needs_signin":
			return m.crew_onboarding_engine_status_sign_in();
		case "ready":
			return m.crew_onboarding_engine_status_ready();
	}
}

/** What the key field of an engine asks for, shown under its name on the engine card ("" for an engine with no key). */
export function agentKeyLabel(agentId: string): string {
	switch (agentId) {
		case "omp":
			return m.crew_onboarding_key_label_omp();
		case "claude":
			return m.crew_onboarding_key_label_claude();
		case "codex":
			return m.crew_onboarding_key_label_codex();
		case "opencode":
			return m.crew_onboarding_key_label_opencode();
		default:
			return "";
	}
}

/** The title over the key field of the sign-in panel: it names the key the engine takes. */
export function pasteKeyTitle(agentId: string): string {
	switch (agentId) {
		case "omp":
			return m.crew_onboarding_paste_key_omp();
		case "claude":
			return m.crew_onboarding_paste_key_claude();
		case "codex":
			return m.crew_onboarding_paste_key_codex();
		case "opencode":
			return m.crew_onboarding_paste_key_opencode();
		default:
			return m.crew_onboarding_paste_key_default();
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
		error: r.installed ? undefined : m.crew_onboarding_engine_install_binary_missing(),
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
export function story(): { title: string; text: string }[] {
	return [
		{ title: m.crew_onboarding_story_order_title(), text: m.crew_onboarding_story_order_text() },
		{ title: m.crew_onboarding_story_server_title(), text: m.crew_onboarding_story_server_text() },
		{ title: m.crew_onboarding_story_approval_title(), text: m.crew_onboarding_story_approval_text() },
		{ title: m.crew_onboarding_story_results_title(), text: m.crew_onboarding_story_results_text() },
	];
}

export type StepMeta = {
	label?: string;
	headline: string;
	body: string;
	bullets?: string[];
	glow: string;
};

/** The headline, body and (info steps) bullets of an onboarding step, in the app's language. */
export function stepMeta(step: OnboardingStep): StepMeta {
	switch (step) {
		case "intro":
			return {
				label: m.crew_onboarding_step_intro_label(),
				headline: m.crew_onboarding_step_intro_headline(),
				body: m.crew_onboarding_step_intro_body(),
				bullets: [
					m.crew_onboarding_step_intro_bullet_server(),
					m.crew_onboarding_step_intro_bullet_sleep(),
					m.crew_onboarding_step_intro_bullet_asks(),
				],
				glow: "rgba(61, 91, 255, 0.10)",
			};
		case "how":
			return {
				label: m.crew_onboarding_step_how_label(),
				headline: m.crew_onboarding_step_how_headline(),
				body: m.crew_onboarding_step_how_body(),
				bullets: undefined,
				glow: "rgba(61, 91, 255, 0.10)",
			};
		case "connect":
			return {
				headline: m.crew_onboarding_step_connect_headline(),
				body: m.crew_onboarding_step_connect_body(),
				glow: "rgba(61, 91, 255, 0.08)",
			};
		case "engine":
			return {
				headline: m.crew_onboarding_step_engine_headline(),
				body: m.crew_onboarding_step_engine_body(),
				glow: "rgba(61, 91, 255, 0.10)",
			};
		case "ready":
			return {
				headline: m.crew_onboarding_step_ready_headline(),
				body: m.crew_onboarding_step_ready_body(),
				glow: "rgba(34, 197, 94, 0.12)",
			};
	}
}
