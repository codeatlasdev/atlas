/**
 * Paints one `Frame` of the globe on a 2D canvas. Stateless: the same `(frame, size, theme)` always gives the same
 * pixels, so the live view and the tests agree.
 *
 * Look: a black stage; the real logo tiles lit as a sphere (key light, rim, specular), crisp radial beams and a ring
 * when a stack comes up.
 */
import { MARK, MARK_VIEW } from "./mark.ts";
import { camera } from "./ribbon.ts";
import { GLOBE, normalAt, shade } from "./shade.ts";
import { TILES, type Frame } from "./timeline.ts";

export interface Theme {
	/** Logo gradient stops (the real Atlas mark: light, mid, deep). */
	light: string;
	mid: string;
	deep: string;
	/** Stage colour behind everything (near-black). */
	stage: string;
}

export const ATLAS: Theme = { light: "#BFD0FF", mid: "#3D5BFF", deep: "#1B33F0", stage: "#03040a" };

/** Path2D per tile id, built once (the data is static). */
let paths: Map<number, Path2D> | null = null;
const pathOf = (id: number) => {
	paths ??= new Map(MARK.map((m) => [m.id, new Path2D(m.d)]));
	return paths.get(id)!;
};

const TILE_BY_ID = new Map(TILES.map((t) => [t.id, t]));
/** Pieces of the logo that never move: the junction nodes of the globe's grid (tiny dots). Drawn once it is whole. */
const STILL = MARK.filter((m) => !TILE_BY_ID.has(m.id));
/** Per-tile brightness from where it sits on the sphere (static, so computed once). */
const LIGHT = new Map(MARK.map((m) => [m.id, shade(normalAt(m.cx, m.cy))]));

/** Fraction of the short side the globe occupies at rest. */
const MARK_FRACTION = 0.46;
/** The beam fan of the hit: angles (radians) and relative lengths, fixed so the burst is the same every run. */
const BEAMS = Array.from({ length: 28 }, (_, i) => {
	const a = (i / 28) * Math.PI * 2 + (i % 2 ? 0.05 : -0.04);
	const len = 0.55 + 0.45 * (((i * 7919) % 97) / 97);
	return { a, len, w: 0.6 + 1.4 * (((i * 104729) % 89) / 89) };
});

/**
 * `w` x `h` are CSS px: the scene is laid out in them under whatever transform the CALLER set (the app sets
 * `setTransform(dpr, 0, 0, dpr, 0, 0)` so a retina buffer is filled; the storyboard uses the identity). `paint` never
 * resets the transform: doing so painted a CSS-sized scene into the top-left quarter of a 2x buffer.
 */
export function paint(
	ctx: CanvasRenderingContext2D,
	f: Frame,
	w: number,
	h: number,
	theme: Theme = ATLAS,
	opts: { shake?: number; layout?: { globeCy: number; globeFraction: number; dim: number } } = {},
): void {
	ctx.globalCompositeOperation = "source-over";
	ctx.globalAlpha = 1;
	ctx.fillStyle = theme.stage;
	ctx.fillRect(0, 0, w, h);

	// The camera: a punch-in and a short shake on the hit, applied to everything on the stage.
	const cam = camera(f.t, opts.shake ?? 1);
	ctx.save();
	ctx.translate(w / 2 + cam.shakeX, h / 2 + cam.shakeY);
	ctx.scale(cam.zoom, cam.zoom);
	ctx.translate(-w / 2, -h / 2);

	const lay = opts.layout ?? { globeCy: 0.44, globeFraction: MARK_FRACTION, dim: 0 };
	const unit = (Math.min(w, h) * lay.globeFraction) / MARK_VIEW.w;
	const scale = unit * f.markScale;
	const cx = w / 2;
	const cy = h * lay.globeCy;
	const R = GLOBE.r * scale;

	// Halo: the globe's own light on the stage.
	{
		const g = ctx.createRadialGradient(cx, cy, R * 0.6, cx, cy, R * 2.6);
		g.addColorStop(0, "rgba(61,91,255,0.22)");
		g.addColorStop(1, "rgba(61,91,255,0)");
		ctx.globalCompositeOperation = "lighter";
		ctx.fillStyle = g;
		ctx.fillRect(0, 0, w, h);
		ctx.globalCompositeOperation = "source-over";
	}
	ctx.save();
	ctx.translate(cx, cy);
	ctx.scale(scale, scale);
	ctx.translate(-MARK_VIEW.w / 2, -MARK_VIEW.h / 2);

	const base = ctx.createLinearGradient(40, 20, 390, 410);
	base.addColorStop(0, theme.light);
	base.addColorStop(0.45, theme.mid);
	base.addColorStop(1, theme.deep);

	for (const s of f.tiles) {
		if (s.alpha <= 0) continue;
		const lit = LIGHT.get(s.id)!;

		// Body: the logo gradient darkened by the lighting of this sheet, brightened while it glows.
		const body = 0.12 + 0.5 * lit.total * (0.35 + 0.65 * Math.min(1, s.glow * 1.6)) + 0.55 * s.glow;
		ctx.globalAlpha = s.alpha;
		ctx.fillStyle = base;
		ctx.fill(pathOf(s.id));
		// Shadow side: multiply-darken the part of the globe facing away from the light.
		ctx.globalCompositeOperation = "source-over";
		ctx.globalAlpha = s.alpha * Math.max(0, 1 - body) * 0.9;
		ctx.fillStyle = "#050824";
		ctx.fill(pathOf(s.id));
		// Light side: additive white-blue on the lit facets, plus the glint.
		ctx.globalCompositeOperation = "lighter";
		ctx.globalAlpha = s.alpha * (0.22 * lit.diffuse + 0.55 * lit.spec + 0.2 * s.glow);
		ctx.fillStyle = "#dfe8ff";
		ctx.fill(pathOf(s.id));
		// Rim: the edge of the globe catches a thin cool light.
		if (lit.rim > 0.08) {
			ctx.globalAlpha = s.alpha * lit.rim * 0.5;
			ctx.lineWidth = (2.4 / scale) * unit * 1.5;
			ctx.strokeStyle = theme.light;
			ctx.stroke(pathOf(s.id));
		}
		// Lit edge while a sheet flares: a bright keyline around the tile.
		if (s.glow > 0.45) {
			ctx.globalAlpha = s.alpha * (s.glow - 0.45) * 1.4;
			ctx.lineWidth = (2.6 / scale) * unit * 1.5;
			ctx.strokeStyle = "#ffffff";
			ctx.stroke(pathOf(s.id));
		}
	}
	// The junction nodes of the grid's wires.
	ctx.globalCompositeOperation = "lighter";
	ctx.globalAlpha = 0.9;
	ctx.fillStyle = "#ffffff";
	for (const m of STILL) ctx.fill(pathOf(m.id));

	ctx.restore();

	// The hit: crisp radial beams of light and a thin sharp ring, not a soft blob.
	if (f.flash > 0.01) {
		ctx.globalCompositeOperation = "lighter";
		const reach = Math.max(w, h) * 0.75;
		for (const b of BEAMS) {
			const len = reach * b.len * (0.5 + 0.9 * (1 - f.flash) ** 0.5);
			const wid = Math.max(0.8, b.w * 2.6 * f.flash);
			const x1 = cx + Math.cos(b.a) * len;
			const y1 = cy + Math.sin(b.a) * len;
			const g = ctx.createLinearGradient(cx, cy, x1, y1);
			g.addColorStop(0, `rgba(255,255,255,${f.flash})`);
			g.addColorStop(0.18, `rgba(255,255,255,${0.7 * f.flash})`);
			g.addColorStop(0.5, `rgba(150,175,255,${0.4 * f.flash})`);
			g.addColorStop(1, "rgba(61,91,255,0)");
			ctx.strokeStyle = g;
			ctx.lineWidth = wid;
			ctx.beginPath();
			ctx.moveTo(cx + Math.cos(b.a) * R * 0.92, cy + Math.sin(b.a) * R * 0.92);
			ctx.lineTo(x1, y1);
			ctx.stroke();
		}
		// A tight core flash: small, white, bright; the globe itself flares.
		const core = ctx.createRadialGradient(cx, cy, 0, cx, cy, R * 1.25);
		core.addColorStop(0, `rgba(255,255,255,${0.55 * f.flash})`);
		core.addColorStop(1, "rgba(120,150,255,0)");
		ctx.fillStyle = core;
		ctx.fillRect(cx - R * 1.3, cy - R * 1.3, R * 2.6, R * 2.6);
	}
	if (f.ring > 0 && f.ring < 1) {
		ctx.globalCompositeOperation = "lighter";
		ctx.strokeStyle = `rgba(200,214,255,${0.7 * (1 - f.ring) ** 2})`;
		ctx.lineWidth = 1.2 + 2.2 * (1 - f.ring);
		ctx.beginPath();
		ctx.arc(cx, cy, R * (1.02 + 3.2 * f.ring), 0, Math.PI * 2);
		ctx.stroke();
	}
	ctx.globalCompositeOperation = "source-over";
	ctx.globalAlpha = 1;

	// Backdrop mode: soften the whole globe toward the stage so the copy under it reads (dim = 0 is the hero, untouched).
	if (lay.dim > 0) {
		ctx.globalCompositeOperation = "source-over";
		ctx.globalAlpha = 1;
		ctx.fillStyle = `rgba(3,4,10,${lay.dim})`;
		ctx.fillRect(0, 0, w, h);
	}
	ctx.restore();
}
