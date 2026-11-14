/** The icon of each automation trigger kind, shared by the automations list and the automation dialog's tabs. */
import type { Component } from "svelte";
import {
	Bot as CrewmateIcon,
	Clock,
	FileText,
	GitPullRequest,
	Link,
	Settings2,
	Timer,
	Zap,
} from "@lucide/svelte";
import type { TriggerKind } from "#lib/crew.ts";

export const TRIGGER_ICONS: Record<TriggerKind, Component> = {
	schedule: Clock,
	interval: Timer,
	webhook: Link,
	github: GitPullRequest,
	file: FileText,
	crewmate: CrewmateIcon,
	service: Settings2,
};

/** Icon of a trigger kind; a kind this build does not know gets a bolt. */
export function triggerIcon(kind: TriggerKind | string): Component {
	return (TRIGGER_ICONS as Record<string, Component>)[kind] ?? Zap;
}
