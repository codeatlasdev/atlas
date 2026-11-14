/**
 * Pure trigger-list edits for the Automations section: each returns the array sent to `crewmate_triggers_set`.
 */
import type { Trigger } from "#lib/crew.ts";

export function withTriggerEnabled(list: Trigger[], id: string, enabled: boolean): Trigger[] {
	return list.map((t) => (t.id === id ? { ...t, enabled } : t));
}

/** Replace the trigger with the same id, or append it when it is new. */
export function upsertTrigger(list: Trigger[], trig: Trigger): Trigger[] {
	return list.some((t) => t.id === trig.id)
		? list.map((t) => (t.id === trig.id ? trig : t))
		: [...list, trig];
}

export function withoutTrigger(list: Trigger[], id: string): Trigger[] {
	return list.filter((t) => t.id !== id);
}
