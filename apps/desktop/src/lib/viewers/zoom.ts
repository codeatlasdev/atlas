/** Pan/zoom math for the image viewer (pure, for tests). */

export const MIN_ZOOM = 0.05;
export const MAX_ZOOM = 32;

export const clampZoom = (z: number) => Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, z));

/** Scale that fits the image in the stage; small images are never upscaled past 100%. */
export function fitScale(img: { w: number; h: number }, box: { w: number; h: number }, pad = 32): number {
	if (!img.w || !img.h || !box.w || !box.h) return 1;
	return Math.min(1, (box.w - pad) / img.w, (box.h - pad) / img.h);
}

/**
 * New pan after zooming from `from` to `to` keeping the point under the cursor fixed.
 * `at` is the cursor relative to the stage center; pan is the image center offset.
 */
export function zoomAt(
	pan: { x: number; y: number },
	at: { x: number; y: number },
	from: number,
	to: number,
): { x: number; y: number } {
	const k = to / from;
	return { x: at.x - (at.x - pan.x) * k, y: at.y - (at.y - pan.y) * k };
}
