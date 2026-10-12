/** Which viewer a file opens in, decided by extension (pure, for tests). */
export type ViewerKind = "image" | "svg" | "pdf" | "video" | "audio" | "markdown" | "text";

export const MIME: Record<string, string> = {
	svg: "image/svg+xml",
	png: "image/png",
	jpg: "image/jpeg",
	jpeg: "image/jpeg",
	gif: "image/gif",
	webp: "image/webp",
	avif: "image/avif",
	ico: "image/x-icon",
	bmp: "image/bmp",
	pdf: "application/pdf",
	mp4: "video/mp4",
	m4v: "video/mp4",
	mov: "video/quicktime",
	webm: "video/webm",
	mp3: "audio/mpeg",
	wav: "audio/wav",
	ogg: "audio/ogg",
	m4a: "audio/mp4",
	aac: "audio/aac",
	flac: "audio/flac",
};

export const extOf = (path: string) => {
	const name = path.split("/").pop() ?? path;
	const dot = name.lastIndexOf(".");
	return dot <= 0 ? "" : name.slice(dot + 1).toLowerCase();
};

const KIND_BY_EXT: Record<string, ViewerKind> = {
	svg: "svg",
	png: "image",
	jpg: "image",
	jpeg: "image",
	gif: "image",
	webp: "image",
	avif: "image",
	bmp: "image",
	ico: "image",
	pdf: "pdf",
	mp4: "video",
	mov: "video",
	webm: "video",
	m4v: "video",
	mp3: "audio",
	wav: "audio",
	ogg: "audio",
	m4a: "audio",
	aac: "audio",
	flac: "audio",
	md: "markdown",
	markdown: "markdown",
};

export const viewerKind = (path: string): ViewerKind => KIND_BY_EXT[extOf(path)] ?? "text";

/** Largest file each kind will load (bytes). Text stays small: CodeMirror holds the whole doc. */
export const MAX_BYTES: Record<ViewerKind, number> = {
	text: 2 * 1024 * 1024,
	markdown: 2 * 1024 * 1024,
	svg: 8 * 1024 * 1024,
	image: 64 * 1024 * 1024,
	pdf: 200 * 1024 * 1024,
	video: 200 * 1024 * 1024,
	audio: 200 * 1024 * 1024,
};

export function formatBytes(n: number): string {
	if (n < 1024) return `${n} B`;
	const u = ["KB", "MB", "GB"];
	let v = n / 1024;
	let i = 0;
	while (v >= 1024 && i < u.length - 1) {
		v /= 1024;
		i++;
	}
	return `${v >= 100 ? v.toFixed(0) : v.toFixed(1)} ${u[i]}`;
}

/** `00000000  48 65 6c 6c 6f …  |Hello|` lines for the first bytes of a binary file. */
export function hexDump(bytes: Uint8Array, width = 16): string {
	const lines: string[] = [];
	for (let off = 0; off < bytes.length; off += width) {
		const row = bytes.subarray(off, off + width);
		const hex = Array.from(row, (b) => b.toString(16).padStart(2, "0"));
		const left = hex.slice(0, width / 2).join(" ");
		const right = hex.slice(width / 2).join(" ");
		const ascii = Array.from(row, (b) => (b >= 0x20 && b < 0x7f ? String.fromCharCode(b) : ".")).join("");
		lines.push(`${off.toString(16).padStart(8, "0")}  ${left.padEnd(23)}  ${right.padEnd(23)}  |${ascii}|`);
	}
	return lines.join("\n");
}
