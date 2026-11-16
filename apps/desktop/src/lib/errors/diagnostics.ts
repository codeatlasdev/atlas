/**
 * The frontend half of the report core (`crates/atlas-desktop/src/diag.rs` is the other): reads the
 * `host_diagnostics` payload defensively, turns a terminal-host failure into an `ErrorReport` the existing
 * error screen already knows how to copy and file, and says which failures offer "Restart terminal host".
 */
import type { Diagnostics, ErrorReport } from "./report.ts";
import { findings, redactText } from "./report.ts";

export { findings };

const str = (v: unknown) => (typeof v === "string" ? v : "");
const strOrNull = (v: unknown) => (typeof v === "string" ? v : null);
const rec = (v: unknown): Record<string, unknown> =>
	v && typeof v === "object" ? (v as Record<string, unknown>) : {};
const num = (v: unknown) => (typeof v === "number" ? v : 0);
const strings = (v: unknown) => (Array.isArray(v) ? v.filter((s): s is string => typeof s === "string") : []);
const rows = (v: unknown) => (Array.isArray(v) ? v.map(rec) : []);

/** `Diagnostics` out of whatever `host_diagnostics` returned; a malformed payload degrades to empty fields. */
export function parseDiagnostics(raw: unknown): Diagnostics {
	const d = rec(raw);
	const v = rec(d.versions);
	return {
		versions: {
			app: str(v.app),
			cli: strOrNull(v.cli),
			hostd: strOrNull(v.hostd),
			hostdPid: typeof v.hostdPid === "number" ? v.hostdPid : null,
			hostdCaps: Array.isArray(v.hostdCaps)
				? v.hostdCaps.filter((c): c is string => typeof c === "string")
				: [],
		},
		os: str(d.os),
		install: {
			runningBundle: strOrNull(rec(d.install).runningBundle),
			installedApp: strOrNull(rec(d.install).installedApp),
			cli: strOrNull(rec(d.install).cli),
			cliLinkedTo: strOrNull(rec(d.install).cliLinkedTo),
			quarantined: rec(d.install).quarantined === true,
			translocated: rec(d.install).translocated === true,
			channel: str(rec(d.install).channel),
		},
		platform: {
			distro: strOrNull(rec(d.platform).distro),
			kernel: strOrNull(rec(d.platform).kernel),
			session: strOrNull(rec(d.platform).session),
			desktop: strOrNull(rec(d.platform).desktop),
			compositor: strOrNull(rec(d.platform).compositor),
			gpus: strings(rec(d.platform).gpus),
			nvidiaDriver: strOrNull(rec(d.platform).nvidiaDriver),
			mesa: strOrNull(rec(d.platform).mesa),
			gtk: strOrNull(rec(d.platform).gtk),
			webview: strOrNull(rec(d.platform).webview),
			graphicsEnv: strings(rec(d.platform).graphicsEnv),
		},
		packaging: {
			appimage: strOrNull(rec(d.packaging).appimage),
			exe: strOrNull(rec(d.packaging).exe),
			exeOwner: strOrNull(rec(d.packaging).exeOwner),
			localBin: strings(rec(d.packaging).localBin),
		},
		crashes: {
			coredumpStatus: str(rec(d.crashes).coredumpStatus),
			coredumps: rows(rec(d.crashes).coredumps).map((r) => ({
				at: num(r.at),
				pid: num(r.pid),
				signal: str(r.signal),
				exe: str(r.exe),
			})),
			atlas: rows(rec(d.crashes).atlas).map((r) => ({
				at: num(r.at),
				ageSecs: num(r.ageSecs),
				summary: str(r.summary),
			})),
		},
		hostdLog: str(d.hostdLog),
		appLog: str(d.appLog),
		stderrLog: str(d.stderrLog),
		lastCrash: strOrNull(d.lastCrash),
		incidents: Array.isArray(d.incidents)
			? d.incidents.map(rec).map((i) => ({
					at: typeof i.at === "number" ? i.at : 0,
					kind: str(i.kind),
					detail: str(i.detail),
				}))
			: [],
	};
}

/** The sentence(s) that precede a host error: what is skewed, if anything. */
export function diagnosticsMarkdown(d: Diagnostics): string {
	return findings(d).join("\n");
}

/** An `ErrorReport` for a terminal-host failure, ready for the existing error screen. */
export function hostErrorReport(message: string, d: Diagnostics): ErrorReport {
	const skew = diagnosticsMarkdown(d);
	return {
		message: redactText(skew ? `${message}\n${skew}` : message),
		stack: "",
		where: "terminal host",
		kind: "hostd",
		version: d.versions.app || undefined,
		context: { diagnostics: d, breadcrumbs: [] },
	};
}

const HOST_FAILURE =
	/terminal host not reachable|hostd (?:is not running|disconnected|did not start|closed)|hostd: unauthorized|hostd\.sock/i;

/** `host` failures get the "Restart terminal host" action and fold into one toast; the rest stay ordinary errors. */
export function classify(message: string): "host" | "other" {
	return HOST_FAILURE.test(message) ? "host" : "other";
}
