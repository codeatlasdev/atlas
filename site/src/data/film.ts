// Chapters, duration and the scrub-sprite grid come from the film definition (studio/films/atlas-1-0/film.ts).
// `bun studio publish atlas-1-0` writes public/media/film-chapters.json next to the video; do not edit the values here.
import film from "../../public/media/film-chapters.json";

export const FILM_DURATION: number = film.duration;
export const THUMB: { step: number; w: number; h: number; cols: number } = film.thumb;
export const CHAPTERS: { t: number; title: string }[] = film.chapters;
