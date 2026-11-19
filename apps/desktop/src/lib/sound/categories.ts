/**
 * The sound categories, alone: ids and the type. No texts and no policy, so the Settings registry's ids can list them
 * without loading the messages `policy.ts` carries.
 */

/** One earcon family member per meaningful event. Ids are also the asset file names. */
export const CATEGORIES = [
	"boot",
	"discovery-done",
	"stack-up",
	"service-failed",
	"agent-needs-you",
	"agent-done",
	"update",
	"error",
	"success",
	"toggle",
] as const;

export type Category = (typeof CATEGORIES)[number];
