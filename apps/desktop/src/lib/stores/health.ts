import type { ProjectView } from "#lib/api.ts";
import type { Live } from "./projects.svelte.ts";

/** Sidebar dot: green all healthy, amber starting/degraded, red failed, grey stopped. */
export function health(
	p: Pick<ProjectView, "running">,
	live: Pick<Live, "services" | "mode"> | undefined,
): "ok" | "warn" | "fail" | "off" {
	if (!p.running) return "off";
	const svcs = (live?.services ?? []).filter((s) => !s.deselected && s.state !== "disabled");
	if (svcs.some((s) => s.state === "failed")) return "fail";
	if (svcs.every((s) => s.state === "running" || s.completed_ok)) {
		// Nothing to run (every service commented out) is up once the runner has reported (`mode` is set).
		if (svcs.length || live?.mode != null) return "ok";
	}
	return "warn";
}
