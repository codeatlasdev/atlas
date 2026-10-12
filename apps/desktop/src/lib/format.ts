/** Memory in MB as `512 MB` / `1.5 GB`. */
export function fmtMem(mb: number): string {
	return mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${mb} MB`;
}

/** Abbreviate the macOS home directory to `~`. */
export function tilde(path: string | null | undefined): string {
	return (path ?? "").replace(/^\/Users\/[^/]+/, "~");
}
