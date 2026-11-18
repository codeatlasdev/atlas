// Chapters, duration and the scrub-sprite grid come from the film definition (studio/films/atlas-1-0/film.ts).
// `bun studio publish atlas-1-0` writes public/media/film-chapters.json next to the video; do not edit the values here.
// The helpers below are the pure functions of that data; the player component keeps only state and DOM.
import film from "../../public/media/film-chapters.json";
import type { Locale } from "../lib/i18n";
import { m } from "../lib/paraglide/messages.js";

export const FILM_DURATION: number = film.duration;
export const THUMB: { step: number; w: number; h: number; cols: number } = film.thumb;
export const CHAPTERS: { t: number; title: string }[] = film.chapters;

/** The ONE rule for every display of the film's length (heading, button, aria-labels, the player's total): whole seconds, rounded. */
export const filmSeconds = (s: number) => (Number.isFinite(s) && s > 0 ? Math.round(s) : 0);

/** `m:ss` of a length. */
export const filmClock = (s: number) => {
	const n = filmSeconds(s);
	return `${Math.floor(n / 60)}:${String(n % 60).padStart(2, "0")}`;
};

/** "1 minute 18 seconds": the spoken form, for aria-labels and alt text. One message holds the plural of each part and the zero cases, which belong to the language; `locale` is for tests and pages that render both languages. */
export const filmWords = (s: number, locale?: Locale) => {
	const n = filmSeconds(s);
	return m.islands_film_duration({ minutes: Math.floor(n / 60), seconds: n % 60 }, locale && { locale });
};

/** `m:ss` of the playhead: floored, so the clock never shows the end before the picture does. */
export const fmt = (s: number) => {
	const n = Number.isFinite(s) && s > 0 ? Math.floor(s) : 0;
	return `${Math.floor(n / 60)}:${String(n % 60).padStart(2, "0")}`;
};

/**
 * What a phone gets. The film is 16:9; in a portrait phone's column it would play in a 360 px box, letterboxed, with
 * controls that do not fit. Until the whole film exists vertically, tapping play on a portrait phone (or small portrait
 * tablet) hands the video to the native fullscreen player (it rotates to landscape and has its own controls), and when
 * the browser cannot do that the player says "rotate your phone" instead of staying silent.
 */
export const mobileFilmUi = (e: { portrait: boolean; width: number; nativeFullscreen: boolean }) => {
	const phone = e.portrait && e.width <= 820;
	return { fullscreenOnPlay: phone && e.nativeFullscreen, rotateHint: phone };
};

/** A 16:9 film in a portrait fullscreen is letterboxed to a strip: ask the visitor to turn the phone (the lock to landscape is not available everywhere). */
export const needsRotate = (e: { fullscreen: boolean; portrait: boolean }) => e.fullscreen && e.portrait;

/** Index of the chapter playing at time `t` (-1 before the first). */
export const chapterAt = (t: number) => CHAPTERS.findLastIndex((c) => t >= c.t);

/** Inline style of the scrub-preview tile for time `t`; `k` scales the sprite down on narrow players (128x72 instead of 160x90). */
export const thumbStyle = (sprite: string, t: number, k: number) => {
	const i = Math.min(Math.floor(t / THUMB.step), Math.ceil(FILM_DURATION / THUMB.step) - 1);
	const w = THUMB.w * k, h = THUMB.h * k;
	return `width:${w}px;height:${h}px;background-image:url(${sprite});background-size:${THUMB.cols * w}px auto;background-position:-${(i % THUMB.cols) * w}px -${Math.floor(i / THUMB.cols) * h}px`;
};
