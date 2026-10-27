/** The camera's punch on the completion hit. Pure; drawn by render.ts. */
import { BEATS } from "./timeline.ts";

export interface Camera {
	/** Zoom factor on top of the mark's own scale (1 = rest). */
	zoom: number;
	/** Shake offset in CSS px, decays quickly. */
	shakeX: number;
	shakeY: number;
}

/** Deterministic noise in -1..1 from a time (no Math.random: screenshots must match the live view). */
const noise = (t: number, seed: number) =>
	Math.sin(t * 91.7 + seed * 12.9898) * Math.cos(t * 53.3 + seed * 78.233);

/** The camera punches in on the hit and shakes for a moment; `shake` scales the shake only (0 = none, for reduced motion). */
export function camera(t: number, shake = 1): Camera {
	const dt = t - BEATS.hit;
	if (!(dt >= 0)) return { zoom: 1, shakeX: 0, shakeY: 0 };
	const decay = Math.exp(-dt * 9);
	return {
		zoom: 1 + 0.06 * Math.exp(-dt * 5) * Math.min(1, dt / 0.03 + 0.001),
		shakeX: shake * 7 * decay * noise(t, 1),
		shakeY: shake * 5 * decay * noise(t, 2),
	};
}
