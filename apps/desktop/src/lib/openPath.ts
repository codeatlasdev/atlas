/** `atlas <path>` / `path_open`: which project a path belongs to and where it is inside it. Pure; the I/O is in ctl.ts. */

export interface Located {
	/** Path of the file relative to the project root (forward slashes). */
	rel: string;
}

/** Forward slashes, no trailing slash: the form Atlas stores project roots in. */
export const slashed = (p: string) => p.replace(/\\/g, "/").replace(/(.)\/+$/, "$1");

/** `file` relative to `root` when it is inside it (case-insensitive on a Windows drive path), else `null`. */
export function relativeTo(root: string, file: string): string | null {
	const r = slashed(root);
	const f = slashed(file);
	const win = /^[a-z]:\//i.test(r);
	const [rc, fc] = win ? [r.toLowerCase(), f.toLowerCase()] : [r, f];
	if (fc === rc) return "";
	return fc.startsWith(`${rc}/`) ? f.slice(r.length + 1) : null;
}
