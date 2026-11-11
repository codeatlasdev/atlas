/** Pure view-model helpers of the project panel: the service rows, ports label and log tickers. */
import type { ProjectConfig, ProjectView, ServiceDef } from "#lib/api.ts";
import type { Service } from "#lib/stores/projects.svelte.ts";
import { byAttention } from "./panel.ts";

export type ServiceRow = Service & { def: ServiceDef | undefined };

/**
 * Rows of the grid, most urgent first: the live services while the stack is up, otherwise
 * the declared ones as stopped/disabled placeholders.
 */
export function serviceRows(defs: ServiceDef[], live: Service[] | undefined, running: boolean): ServiceRow[] {
	const byName: Record<string, ServiceDef> = Object.fromEntries(defs.map((s) => [s.name, s]));
	const rows =
		running && live?.length
			? live
			: defs.map((s): Service => ({
					name: s.name,
					state: s.enabled ? "stopped" : "disabled",
					exit_code: null,
					restarts: 0,
					note: null,
				}));
	return byAttention(rows).map((s) => ({ ...s, def: byName[s.name] }));
}

/** `:3000 :3001 … +2`: the first five enabled service ports. */
export function portsLabel(defs: ServiceDef[]): string {
	const ports = defs.flatMap((s) => (s.enabled && s.port ? [s.port] : []));
	if (!ports.length) return "";
	return `${ports
		.slice(0, 5)
		.map((x) => `:${x}`)
		.join(" ")}${ports.length > 5 ? ` +${ports.length - 5}` : ""}`;
}

/** Last non-marker log line of each of `names`, scanning `items` (oldest first) from the end. */
export function lastLines(
	items: readonly { name: string; line: string; mark?: unknown }[],
	names: string[],
): Record<string, string> {
	const want = new Set(names);
	const out: Record<string, string> = {};
	for (let i = items.length - 1; i >= 0 && want.size; i--) {
		const l = items[i];
		if (!l.mark && want.delete(l.name)) out[l.name] = l.line;
	}
	return out;
}

/** `a, b, c, d, +2`: up to four names of the stack being started. */
export function namesLabel(names: string[]): string {
	return `${names.slice(0, 4).join(", ")}${names.length > 4 ? `, +${names.length - 4}` : ""}`;
}

/**
 * What the panel shows. `services` and `loading`/`broken` never produce the "nothing to run" copy: the grid shows the
 * services, a config that is still loading says nothing, and a broken atlas.yaml is the banner's to explain.
 */
export type PanelEmpty =
	| { kind: "services" | "loading" | "broken" | "nothing" | "empty-file" }
	| { kind: "detected"; detected: string };

export function emptyPanel(
	project: Pick<ProjectView, "config_source" | "error">,
	cfg: Pick<ProjectConfig, "services" | "detected"> | undefined,
): PanelEmpty {
	if (project.error) return { kind: "broken" };
	if (!cfg) return { kind: "loading" };
	if (cfg.services.length) return { kind: "services" };
	if (project.config_source === "file") return { kind: "empty-file" };
	return cfg.detected ? { kind: "detected", detected: cfg.detected } : { kind: "nothing" };
}
