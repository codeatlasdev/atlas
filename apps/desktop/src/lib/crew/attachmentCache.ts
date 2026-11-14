/**
 * The bytes of the images in the thread, fetched once per image per session: the thread re-renders on every push
 * and a task can show in the thread and on the Board, and each fetch is up to ten round trips to a crew server.
 * Bounded (a long chat must not keep every screenshot in memory); a failed fetch is forgotten so the next render
 * retries.
 */
import { fetchAttachment, type AttachmentRef, type HostCall, type LoadedImage } from "./attachments.ts";

/** Images kept; the oldest is dropped past this. */
const MAX_CACHED = 32;

const cache = new Map<string, Promise<LoadedImage>>();

/** The image under `key`, fetching it with `call` on first use. `ref` is only evaluated when a fetch starts. */
export function cachedAttachment(
	key: string,
	ref: () => AttachmentRef,
	call: HostCall,
): Promise<LoadedImage> {
	const hit = cache.get(key);
	if (hit) return hit;
	const p = fetchAttachment(call, ref());
	cache.set(key, p);
	p.catch(() => cache.get(key) === p && cache.delete(key));
	if (cache.size > MAX_CACHED) cache.delete(cache.keys().next().value as string);
	return p;
}

/** Forget everything (tests; a server switch). */
export function clearAttachmentCache(): void {
	cache.clear();
}
