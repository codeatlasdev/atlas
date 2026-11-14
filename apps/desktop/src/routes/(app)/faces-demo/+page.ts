import { error } from "@sveltejs/kit";

// Gallery for screenshots and the Atlas Lab; not part of the shipped app.
export function load() {
	if (!import.meta.env.DEV) error(404, "Not found");
}
