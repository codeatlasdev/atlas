/**
 * The machine, packaging and crash lines of a report (#275): distro, session, GPU, graphics stack,
 * how Atlas was installed, core dumps and Atlas crash files. `machine_lines` in
 * `crates/atlas-host/src/bundle.rs` prints the same text for the same payload (`atlas doctor
 * --bundle`); `machine.test.ts` compares the two on the fixtures Rust wrote.
 */
import type { Diagnostics } from "./report.ts";

/** `2026-11-15 10:00:00 UTC` from unix seconds. */
const iso = (secs: number) => `${new Date(secs * 1000).toISOString().slice(0, 19).replace("T", " ")} UTC`;

/** `45s`, `12m`, `5h`, `3d`. */
function age(secs: number): string {
	if (secs < 60) return `${secs}s`;
	if (secs < 3600) return `${Math.floor(secs / 60)}m`;
	if (secs < 86_400) return `${Math.floor(secs / 3600)}h`;
	return `${Math.floor(secs / 86_400)}d`;
}

export function machineLines(d: Diagnostics): string[] {
	const { platform: p, packaging: k, crashes: c } = d;
	const opt = (label: string, v: string | null) => (v ? [`${label}: ${v}`] : []);
	const session = [
		p.session,
		p.desktop && `desktop ${p.desktop}`,
		p.compositor && `compositor ${p.compositor}`,
	].filter(Boolean);
	const core =
		c.coredumpStatus === ""
			? []
			: c.coredumpStatus !== "ok"
				? [`Core dumps (24h): ${c.coredumpStatus}`]
				: c.coredumps.length === 0
					? ["Core dumps (24h): none"]
					: [
							`Core dumps (24h): ${c.coredumps.length}`,
							...c.coredumps.map((r) => `  ${iso(r.at)} ${r.signal} pid ${r.pid} ${r.exe}`),
						];
	const newest = c.atlas[0];
	return [
		...opt("Distro", p.distro),
		...opt("Kernel", p.kernel),
		...(session.length ? [`Session: ${session.join(", ")}`] : []),
		...p.gpus.map((g) => `GPU: ${g}`),
		...opt("NVIDIA driver", p.nvidiaDriver),
		...opt("Mesa", p.mesa),
		...opt("GTK", p.gtk),
		...opt("Web view", p.webview),
		...(p.graphicsEnv.length ? [`Graphics env: ${p.graphicsEnv.join(" ")}`] : []),
		...opt("AppImage", k.appimage),
		...(k.exe ? [`Binary: ${k.exe}${k.exeOwner ? ` (package: ${k.exeOwner})` : ""}`] : []),
		...(k.localBin.length ? [`~/.local/bin: ${k.localBin.join(", ")}`] : []),
		...core,
		newest
			? `Atlas crash reports: newest ${iso(newest.at)} (${age(newest.ageSecs)} ago)`
			: "Atlas crash reports: none",
	];
}
