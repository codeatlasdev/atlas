/**
 * Images on a crewmate task (#209), the client half of `crates/atlas-host/src/crew/attachments.rs`: what the
 * composer accepts (the same caps and the same by-the-bytes type check as hostd, so a refusal shows before the
 * upload), how an image goes on the wire, and how the thread gets its bytes back.
 *
 * A protocol line is at most 1 MiB, so an image too big for one rides in pieces (`crewmate_attachment_put`) and
 * is named by its upload id in `crewmate_task`; a small one goes inline. Fetching is in pieces too.
 */
import { b64bytes, b64encode } from "#lib/b64.ts";

/** Images a task takes. */
export const MAX_ATTACHMENTS = 4;
/** The largest image, in bytes. */
export const MAX_BYTES = 5 * 1024 * 1024;
/** Raw bytes per protocol line (hostd's `attachments::CHUNK`). */
export const CHUNK = 512 * 1024;

/** An image kept with a task: what `crewmate_tasks` returns. Never carries the bytes. */
export interface Attachment {
	name: string;
	mime: string;
	bytes_len: number;
	sha256: string;
	path: string;
	/** "agent" for a capture the crewmate took; absent for an image the owner sent. */
	source?: "agent";
}

/** An image waiting in the composer. */
export interface PendingImage {
	name: string;
	mime: string;
	bytes: Uint8Array;
}

/** One image of a `crewmate_task`: inline bytes or the id of an upload. */
export type SentImage =
	{ name: string; mime: string; data_b64: string } | { name: string; mime: string; upload: string };

/** A host command call: `hostCall` in the app, a fake in tests. */
export type HostCall = (cmd: string, args: Record<string, unknown>) => Promise<unknown>;

const startsWith = (b: Uint8Array, sig: number[], at = 0) => sig.every((v, i) => b[at + i] === v);
const ascii = (s: string) => [...s].map((c) => c.charCodeAt(0));

/** The image kind of `bytes` by its signature; null for anything else (an SVG, a PDF, a renamed script). */
export function sniff(b: Uint8Array): string | null {
	if (startsWith(b, [0x89, ...ascii("PNG"), 0x0d, 0x0a, 0x1a, 0x0a])) return "image/png";
	if (startsWith(b, [0xff, 0xd8, 0xff])) return "image/jpeg";
	if (startsWith(b, ascii("GIF87a")) || startsWith(b, ascii("GIF89a"))) return "image/gif";
	if (b.length >= 12 && startsWith(b, ascii("RIFF")) && startsWith(b, ascii("WEBP"), 8)) return "image/webp";
	return null;
}

const EXT: Record<string, string> = {
	"image/png": "png",
	"image/jpeg": "jpg",
	"image/gif": "gif",
	"image/webp": "webp",
};

/**
 * `files` added to the `pending` images: the ones that pass, and a message for each that did not (not an image
 * by its bytes, empty, over 5 MiB, or past the fourth). Order is kept; nothing is dropped silently.
 */
export async function addImages(
	pending: PendingImage[],
	files: File[],
): Promise<{ images: PendingImage[]; errors: string[] }> {
	const images = [...pending];
	const errors: string[] = [];
	for (const f of files) {
		const shown = f.name || "image";
		if (images.length >= MAX_ATTACHMENTS) {
			errors.push(`A task takes at most ${MAX_ATTACHMENTS} images: ${shown} was not added`);
			continue;
		}
		if (f.size > MAX_BYTES) {
			errors.push(`${shown} is over ${MAX_BYTES / (1024 * 1024)} MiB`);
			continue;
		}
		const bytes = new Uint8Array(await f.arrayBuffer());
		const mime = sniff(bytes);
		if (!mime) {
			errors.push(`${shown} is not a png, jpeg, webp or gif image`);
			continue;
		}
		// A pasted screenshot has no name.
		const name = f.name || `pasted-${Date.now()}-${images.length}.${EXT[mime]}`;
		images.push({ name, mime, bytes });
	}
	return { images, errors };
}

/** The image files of a paste event (a screenshot copied to the clipboard); text pastes yield none. */
export function imagesFromClipboard(e: {
	clipboardData: { items: ArrayLike<DataTransferItem> } | null;
}): File[] {
	const files: File[] = [];
	for (const item of Array.from(e.clipboardData?.items ?? [])) {
		const f = item.kind === "file" && item.type.startsWith("image/") ? item.getAsFile() : null;
		if (f) files.push(f);
	}
	return files;
}

/** Binary string of base64 that fits one protocol line for `bytes` (4/3 of them) with room for the rest. */
const INLINE_MAX = 256 * 1024;

/**
 * The `attachments` of a `crewmate_task`. An image of at most 256 KiB goes inline; a bigger one is uploaded first,
 * piece by piece in order (`extra` is merged into every call: the crewmate `id` and the `host` route), and named by
 * the upload id `newId()` gave it.
 */
export async function toSent(
	images: PendingImage[],
	call: HostCall,
	newId: () => string,
	extra: Record<string, unknown> = {},
): Promise<SentImage[]> {
	const out: SentImage[] = [];
	for (const im of images) {
		if (im.bytes.length <= INLINE_MAX) {
			out.push({ name: im.name, mime: im.mime, data_b64: b64encode(im.bytes) });
			continue;
		}
		const upload = newId();
		const pieces = Math.ceil(im.bytes.length / CHUNK);
		for (let seq = 0; seq < pieces; seq++) {
			await call("crewmate_attachment_put", {
				...extra,
				upload,
				seq,
				data_b64: b64encode(im.bytes.subarray(seq * CHUNK, (seq + 1) * CHUNK)),
				last: seq === pieces - 1,
			});
		}
		out.push({ name: im.name, mime: im.mime, upload });
	}
	return out;
}

/** Which stored image to fetch, plus the route every host call carries (`host`). */
export type AttachmentRef = { id: string; task: string; name: string } & Record<string, unknown>;

/** A fetched image: its type (from hostd, which checked the bytes) and the bytes. */
export interface LoadedImage {
	mime: string;
	bytes: Uint8Array;
}

interface Piece {
	name: string;
	mime: string;
	bytes_len: number;
	offset: number;
	data_b64: string;
	last: boolean;
}

/** The bytes of a stored image: `crewmate_attachment` until `last`, the offset advancing by what arrived. */
export async function fetchAttachment(call: HostCall, ref: AttachmentRef): Promise<LoadedImage> {
	const parts: Uint8Array[] = [];
	let got = 0;
	let mime = "";
	// A piece is up to CHUNK bytes and an image at most MAX_BYTES: more reads than that is a server that never ends.
	for (let reads = 0; reads <= MAX_BYTES / CHUNK + 1; reads++) {
		const p = (await call("crewmate_attachment", { ...ref, offset: got })) as Piece;
		const bytes = b64bytes(p.data_b64);
		parts.push(bytes);
		got += bytes.length;
		mime = p.mime;
		if (p.last) {
			const all = new Uint8Array(got);
			let at = 0;
			for (const part of parts) {
				all.set(part, at);
				at += part.length;
			}
			return { mime, bytes: all };
		}
		if (bytes.length === 0 || got > MAX_BYTES) break;
	}
	throw new Error(`${ref.name}: the server never finished sending it`);
}
