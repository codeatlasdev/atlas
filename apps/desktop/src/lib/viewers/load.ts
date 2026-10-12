import { b64bytes, hostCall } from "#lib/host.ts";

const CHUNK = 8 * 1024 * 1024;

/**
 * Reads a whole file through hostd in 8 MB chunks (`fs_read` with `offset`) into a Blob.
 * `onProgress` gets 0..1. Throws when the host ignores `offset` (an older hostd would return
 * the head again and corrupt the file).
 */
export async function readBlob(
	path: string,
	size: number,
	type: string,
	onProgress?: (p: number) => void,
	signal?: AbortSignal,
): Promise<Blob> {
	const parts: Uint8Array<ArrayBuffer>[] = [];
	let offset = 0;
	while (offset < size) {
		if (signal?.aborted) throw new DOMException("aborted", "AbortError");
		const r = await hostCall<{ data: string; truncated: boolean; offset?: number }>("fs_read", {
			path,
			max: CHUNK,
			offset,
		});
		if (offset > 0 && r.offset !== offset)
			throw new Error("This host is too old to stream big files; update atlas on it.");
		const bytes = b64bytes(r.data) as Uint8Array<ArrayBuffer>;
		if (!bytes.length) break;
		parts.push(bytes);
		offset += bytes.length;
		onProgress?.(Math.min(1, offset / size));
		if (!r.truncated) break;
	}
	return new Blob(parts, { type });
}
