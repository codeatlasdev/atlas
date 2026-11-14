/** The foreign process holding a service's port (runner `port_conflict`, atlas-tui `PortConflict`). */
export interface PortConflict {
	pid: number;
	cmd: string;
	cwd: string | null;
}

/** Value compare: `state` events repeat on every health tick with a fresh object each time. */
export function sameConflict(a?: PortConflict | null, b?: PortConflict | null): boolean {
	if (!a || !b) return !a === !b;
	return a.pid === b.pid && a.cmd === b.cmd && a.cwd === b.cwd;
}
