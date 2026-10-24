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

/** Which piece was just put on (BotFace `react`). `kit` = several at once. */
export type EquipPart = "hat" | "outfit" | "tool" | "face" | "kit" | "color" | "eyes";

/**
 * Velocity kicks added to the sprung channels (index-aligned with toChannels) when
 * something is put on: a small happy hop with a squash on landing, hands flapping up.
 * Kicks, not targets, so the hop overlays whatever the state is doing.
 */
export function equipImpulse(part: EquipPart): number[] {
	const kick = new Array<number>(STIFFNESS.length).fill(0);
	const big = part === "kit" ? 1.35 : 1;
	kick[0] = -1500 * big; // body y: up
	kick[1] = -1.1 * big; // sx: stretch thin on the way up
	kick[2] = 1.5 * big; // sy: and tall
	kick[4] = part === "hat" ? 900 : 260 * big; // hat trails (and bounces hardest when it is the new piece)
	kick[6] = -900 * big; // left hand
	kick[9] = part === "tool" ? -1400 : -900 * big; // right hand: tossed up when the tool is new
	kick[11] = part === "tool" ? 500 : 0; // tool swings from the catch
	return kick;
}

/**
 * Entry animation of a freshly put-on piece, `k` = seconds since it was equipped.
 * Pure, in SVG units/fractions, so BotFace only samples it each frame.
 *  - hat: drops in from above with a bounce
 *  - outfit: wipes on top to bottom (fraction revealed, 0..1)
 *  - tool: tossed in from above-right, settling into the hand
 *  - face: snaps on (scale overshoot) with a small fade
 */
export function equipEnter(
	part: "hat" | "outfit" | "tool" | "face",
	k: number,
): { dy: number; dx: number; reveal: number; scale: number; alpha: number } {
	const out = { dy: 0, dx: 0, reveal: 1, scale: 1, alpha: 1 };
	if (k < 0) return out;
	switch (part) {
		case "hat": {
			const dur = 0.7;
			if (k >= dur) return out;
			const u = k / dur;
			// Fall with gravity, bounce twice with decaying height.
			const fall = 0.4;
			if (u < fall) {
				const f = u / fall;
				out.dy = -300 * (1 - f * f);
			} else {
				const b = (u - fall) / (1 - fall);
				out.dy = -40 * Math.abs(Math.sin(b * Math.PI * 2)) * (1 - b);
			}
			out.alpha = Math.min(1, k / 0.08);
			return out;
		}
		case "outfit": {
			const dur = 0.5;
			if (k >= dur) return out;
			const u = k / dur;
			out.reveal = 1 - (1 - u) ** 3;
			return out;
		}
		case "tool": {
			const dur = 0.65;
			if (k >= dur) return out;
			const u = k / dur;
			const e = 1 - (1 - u) ** 3;
			// Arc in from above-right, tiny overshoot via the sin term.
			out.dx = 160 * (1 - e);
			out.dy = -260 * (1 - e) - Math.sin(u * Math.PI) * 50;
			out.scale = 0.4 + 0.6 * e;
			out.alpha = Math.min(1, k / 0.06);
			return out;
		}
		case "face": {
			const dur = 0.4;
			if (k >= dur) return out;
			const u = k / dur;
			// Back-ease: overshoots past 1 then settles.
			const c = 2.4;
			const e = 1 + (c + 1) * (u - 1) ** 3 + c * (u - 1) ** 2;
			out.scale = 1.6 - 0.6 * e;
			out.alpha = Math.min(1, k / 0.05);
			return out;
		}
	}
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
