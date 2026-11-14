import { error } from "@sveltejs/kit";

// Ship-scene preview for screenshots; not part of the shipped app.
export function load() {
	if (!import.meta.env.DEV) error(404, "Not found");
}
