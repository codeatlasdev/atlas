// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { health } from "./health.ts";

const live = (services, mode = "runner") => ({ services, mode });
const row = (state, o = {}) => ({ name: state, state, ...o });

// Incident (Windows, first run): a project whose atlas.yaml has every service commented out showed an orange
// "0 of 0 up" warning forever. A running stack with nothing to run is healthy once its runner has reported.
test("a running stack with no services is ok once the runner reported, and warn before that", () => {
	expect(health({ running: true }, live([]))).toBe("ok");
	expect(health({ running: true }, live([row("stopped", { deselected: true })]))).toBe("ok");
	expect(health({ running: true }, { services: [], mode: null })).toBe("warn");
	expect(health({ running: true }, undefined)).toBe("warn");
});

test("health: failed beats everything, all up is ok, anything starting is warn, stopped stack is off", () => {
	expect(health({ running: true }, live([row("running"), row("failed")]))).toBe("fail");
	expect(health({ running: true }, live([row("running"), row("stopped", { completed_ok: true })]))).toBe(
		"ok",
	);
	expect(health({ running: true }, live([row("running"), row("starting")]))).toBe("warn");
	expect(health({ running: false }, live([row("running")]))).toBe("off");
});
