/**
 * The crewmate's motion rig: where the body, hat and both hands want to be at
 * time `t` (seconds) for a given state and tool. Pure, no DOM: BotFace springs
 * its drawn pose toward this target every frame, so state changes overshoot
 * and settle instead of snapping, and hands/hat trail the body (secondary motion).
 *
 * Coordinates are the face's SVG space: body centre (214, 213), radius 206,
 * floor at y≈430. Hands rest just outside the shell, a little below the middle.
 */
import type { BotState } from "./botFace.ts";
import type { Tool } from "./look.ts";

export interface Hand {
	x: number;
	y: number;
	/** degrees */
	r: number;
}

export interface Pose {
	/** body lift (negative = up) */
	y: number;
	/** squash/stretch around the floor */
	sx: number;
	sy: number;
	/** body lean, degrees */
	r: number;
	hat: number;
	left: Hand;
	right: Hand;
	/** free channel for the tool itself (compass needle angle, lantern swing, megaphone pulse) */
	tool: number;
}

export const REST_LEFT: Hand = { x: 14, y: 300, r: 0 };
export const REST_RIGHT: Hand = { x: 414, y: 300, r: 0 };

const TAU = Math.PI * 2;
const wave = (t: number, period: number, phase = 0) => Math.sin((t / period) * TAU + phase);

/** The right hand's "using the tool" gesture while working. */
function workGesture(tool: Tool, t: number): { right: Hand; tool: number } {
	switch (tool) {
		case "wrench": {
			// Turn a bolt: quarter swings with a pause at the end of each stroke.
			const s = wave(t, 0.9);
			return { right: { x: 380, y: 250 + s * 10, r: s * 38 }, tool: 0 };
		}
		case "quill": {
			// Scribble: fast small loops low in front of the body.
			return {
				right: { x: 330 + wave(t, 0.5) * 22, y: 330 + wave(t, 0.25) * 8, r: -20 + wave(t, 0.5) * 6 },
				tool: 0,
			};
		}
		case "spyglass":
		case "magnifier": {
			// Up to the eye and sweep the horizon.
			return {
				right: { x: 300 + wave(t, 2.4) * 28, y: 190 + wave(t, 1.2) * 6, r: -10 + wave(t, 2.4) * 8 },
				tool: 0,
			};
		}
		case "megaphone": {
			// Raised, pulsing with each call.
			const p = Math.max(0, wave(t, 0.7));
			return { right: { x: 360, y: 230 - p * 10, r: -18 - p * 6 }, tool: p };
		}
		case "compass": {
			// Held out front; the needle hunts for north.
			return {
				right: { x: 360, y: 280 + wave(t, 1.6) * 6, r: 0 },
				tool: wave(t, 2.2) * 60 + wave(t, 0.55) * 12,
			};
		}
		case "pin": {
			// Plant the pin, lift, plant again.
			const d = Math.max(0, wave(t, 1.1));
			return { right: { x: 390, y: 260 + d * 40, r: d * 12 }, tool: d };
		}
		case "lantern": {
			return { right: { x: 400, y: 240, r: 0 }, tool: wave(t, 1.3) * 18 };
		}
		default: {
			// Empty hand: typing in the air.
			return { right: { x: 360, y: 320 + Math.abs(wave(t, 0.35)) * -14, r: 0 }, tool: 0 };
		}
	}
}

export function pose(state: BotState, tool: Tool, t: number): Pose {
	const breath = wave(t, 3.6);
	const idle: Pose = {
		y: 0,
		sx: 1 + breath * 0.008,
		sy: 1 + breath * 0.018,
		r: 0,
		hat: wave(t, 3.6, -0.6) * 1.5,
		left: { x: REST_LEFT.x, y: REST_LEFT.y + breath * 4, r: 0 },
		right: { x: REST_RIGHT.x, y: REST_RIGHT.y - breath * 4, r: 0 },
		tool: tool === "compass" ? wave(t, 6) * 15 : tool === "lantern" ? wave(t, 3.6) * 6 : 0,
	};

	if (state === "offline") {
		// Slumped: slower breath, hands drop, hat slides over the eyes.
		const b = wave(t, 5.5);
		return {
			...idle,
			y: 12,
			sx: 1.02 + b * 0.006,
			sy: 0.97 + b * 0.014,
			r: -4,
			hat: 8,
			left: { x: 30, y: 360, r: 20 },
			right: { x: 398, y: 360, r: -20 },
			tool: 0,
		};
	}

	if (state === "working") {
		const g = workGesture(tool, t);
		return {
			...idle,
			y: -4 + wave(t, 1.4) * 4,
			r: wave(t, 2.8) * 2,
			hat: wave(t, 1.4, -0.5) * 3,
			left: { x: 40, y: 290 + wave(t, 1.4) * 6, r: 0 },
			right: g.right,
			tool: g.tool,
		};
	}

	if (state === "needs_you") {
		// Hop and wave the free hand overhead; the tool hand bounces along.
		const phase = (t % 0.9) / 0.9;
		const air = Math.sin(Math.min(1, phase / 0.7) * Math.PI);
		const land = phase > 0.7 ? Math.sin(((phase - 0.7) / 0.3) * Math.PI) : 0;
		return {
			...idle,
			y: -46 * air,
			sx: 1 + land * 0.07 - air * 0.03,
			sy: 1 - land * 0.07 + air * 0.04,
			r: 0,
			hat: -air * 6,
			left: { x: -20, y: 40 + wave(t, 0.45) * 14, r: wave(t, 0.45) * 30 },
			right: { x: REST_RIGHT.x, y: REST_RIGHT.y - air * 30, r: 0 },
			tool: idle.tool,
		};
	}

	return idle;
}

/**
 * Critically-underdamped spring toward `target`, one channel. Returns the new
 * [value, velocity]. Stiffness/damping tuned for a soft overshoot (~8%).
 */
export function spring(
	value: number,
	velocity: number,
	target: number,
	dt: number,
	k = 170,
	c = 16,
): [number, number] {
	// Sub-step so a dropped frame cannot explode the integration.
	let v = velocity;
	let x = value;
	const steps = Math.max(1, Math.ceil(dt / (1 / 120)));
	const h = dt / steps;
	for (let i = 0; i < steps; i++) {
		v += (-k * (x - target) - c * v) * h;
		x += v * h;
	}
	return [x, v];
}

/** Pose ↔ flat channels, so BotFace can spring every number with one loop. */
export function toChannels(p: Pose): number[] {
	return [p.y, p.sx, p.sy, p.r, p.hat, p.left.x, p.left.y, p.left.r, p.right.x, p.right.y, p.right.r, p.tool];
}

export function fromChannels(c: readonly number[]): Pose {
	return {
		y: c[0],
		sx: c[1],
		sy: c[2],
		r: c[3],
		hat: c[4],
		left: { x: c[5], y: c[6], r: c[7] },
		right: { x: c[8], y: c[9], r: c[10] },
		tool: c[11],
	};
}

/** Body channels are snappy; hands and hat trail behind (follow-through). Index-aligned with toChannels. */
export const STIFFNESS: readonly [number, number][] = [
	[260, 18],
	[300, 16],
	[300, 16],
	[140, 14], // body
	[70, 7], // hat: loose, overshoots
	[150, 15],
	[150, 15],
	[120, 12], // left hand
	[170, 16],
	[170, 16],
	[140, 13], // right hand
	[90, 10], // tool
];
