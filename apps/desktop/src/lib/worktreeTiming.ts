import type { SeedReport } from "#lib/api.ts";

/** One line for what seeding did (`atlas_core::seed::Report`); null when there is nothing worth saying (missing or existing dirs are normal). */
export function seedLine(r: SeedReport | null | undefined): string | null {
	if (!r) return null;
	const parts: string[] = [];
	if (r.seeded.length) parts.push(`cloned ${r.seeded.join(", ")} in ${r.ms} ms`);
	const unsupported = r.skipped.filter(([, s]) => s.reason === "unsupported").map(([d]) => d);
	if (unsupported.length) parts.push(`${unsupported.join(", ")} skipped (this file system cannot clone)`);
	for (const [dir, s] of r.skipped) {
		if (s.reason === "failed") parts.push(`${dir}: ${s.detail ?? "failed"}`);
		if (s.reason === "not_relocatable")
			parts.push(`${dir} not seeded (a Python virtualenv is not relocatable: recreate it)`);
	}
	return parts.length ? parts.join("; ") : null;
}

function duration(ms: number): string {
	return ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1)} s`;
}

/** The "ready" report: seed time, then how long the Setup command ran (null until it ended). */
export function readyLine(seed: SeedReport | null | undefined, setupMs: number | null): string | null {
	const parts = [seedLine(seed), setupMs === null ? null : `setup ${duration(setupMs)}`];
	const line = parts.filter(Boolean).join(" · ");
	return line || null;
}
