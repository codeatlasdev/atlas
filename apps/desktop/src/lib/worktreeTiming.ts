import { formatNumber } from "#lib/i18n/fmt.ts";
import { m } from "#lib/i18n/m.ts";
import type { SeedReport } from "#lib/api.ts";

/** One line for what seeding did (`atlas_core::seed::Report`); null when there is nothing worth saying (missing or existing dirs are normal). */
export function seedLine(r: SeedReport | null | undefined): string | null {
	if (!r) return null;
	const parts: string[] = [];
	if (r.seeded.length) parts.push(m.shell_toasts_seed_cloned({ dirs: r.seeded.join(", "), ms: r.ms }));
	const unsupported = r.skipped.filter(([, s]) => s.reason === "unsupported").map(([d]) => d);
	if (unsupported.length) parts.push(m.shell_toasts_seed_unsupported({ dirs: unsupported.join(", ") }));
	for (const [dir, s] of r.skipped) {
		if (s.reason === "failed") parts.push(`${dir}: ${s.detail ?? m.shell_toasts_seed_failed()}`);
		if (s.reason === "not_relocatable") parts.push(m.shell_toasts_seed_not_relocatable({ dir }));
	}
	return parts.length ? parts.join("; ") : null;
}

function duration(ms: number): string {
	return ms < 1000
		? `${formatNumber(ms)} ms`
		: `${formatNumber(ms / 1000, { minimumFractionDigits: 1, maximumFractionDigits: 1 })} s`;
}

/** The "ready" report: seed time, then how long the Setup command ran (null until it ended). */
export function readyLine(seed: SeedReport | null | undefined, setupMs: number | null): string | null {
	const parts = [
		seedLine(seed),
		setupMs === null ? null : m.shell_toasts_setup_time({ duration: duration(setupMs) }),
	];
	const line = parts.filter(Boolean).join(" · ");
	return line || null;
}
