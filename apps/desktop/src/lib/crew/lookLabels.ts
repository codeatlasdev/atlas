/**
 * The words of a crewmate's look (the crewmate editor's option names and one-click kits). Kept apart from look.ts,
 * which the sidebar and every avatar import at startup: only the editor needs these messages, and it loads later.
 *
 * The labels are catalog messages read through getters, so a language switch reaches these tables without a rebuild.
 */
import { m } from "#lib/i18n/m.ts";
import type { CrewmateLook, EyeStyle, Face, Hat, Outfit, Tool } from "#lib/crew/look.ts";

export const EYE_LABELS: Record<EyeStyle, string> = {
	get round() {
		return m.crew_workspace_look_eyes_round();
	},
	get dots() {
		return m.crew_workspace_look_eyes_dots();
	},
	get happy() {
		return m.crew_workspace_look_eyes_happy();
	},
	get visor() {
		return m.crew_workspace_look_eyes_visor();
	},
	get cyclops() {
		return m.crew_workspace_look_eyes_cyclops();
	},
	get sleepy() {
		return m.crew_workspace_look_eyes_sleepy();
	},
};

export const HAT_LABELS: Record<Hat, string> = {
	get none() {
		return m.crew_workspace_look_hat_none();
	},
	get straw() {
		return m.crew_workspace_look_hat_straw();
	},
	get cap() {
		return m.crew_workspace_look_hat_cap();
	},
	get beanie() {
		return m.crew_workspace_look_hat_beanie();
	},
	get tophat() {
		return m.crew_workspace_look_hat_tophat();
	},
	get crown() {
		return m.crew_workspace_look_hat_crown();
	},
	get party() {
		return m.crew_workspace_look_hat_party();
	},
	get wizard() {
		return m.crew_workspace_look_hat_wizard();
	},
	get headphones() {
		return m.crew_workspace_look_hat_headphones();
	},
	get antenna() {
		return m.crew_workspace_look_hat_antenna();
	},
	get bicorne() {
		return m.crew_workspace_look_hat_bicorne();
	},
	get bandana() {
		return m.crew_workspace_look_hat_bandana();
	},
};

export const TOOL_LABELS: Record<Tool, string> = {
	get none() {
		return m.crew_workspace_look_tool_none();
	},
	get compass() {
		return m.crew_workspace_look_tool_compass();
	},
	get spyglass() {
		return m.crew_workspace_look_tool_spyglass();
	},
	get wrench() {
		return m.crew_workspace_look_tool_wrench();
	},
	get quill() {
		return m.crew_workspace_look_tool_quill();
	},
	get magnifier() {
		return m.crew_workspace_look_tool_magnifier();
	},
	get megaphone() {
		return m.crew_workspace_look_tool_megaphone();
	},
	get pin() {
		return m.crew_workspace_look_tool_pin();
	},
	get lantern() {
		return m.crew_workspace_look_tool_lantern();
	},
};

export const OUTFIT_LABELS: Record<Outfit, string> = {
	get none() {
		return m.crew_workspace_look_outfit_none();
	},
	get scarf() {
		return m.crew_workspace_look_outfit_scarf();
	},
	get coat() {
		return m.crew_workspace_look_outfit_coat();
	},
	get apron() {
		return m.crew_workspace_look_outfit_apron();
	},
	get satchel() {
		return m.crew_workspace_look_outfit_satchel();
	},
	get tie() {
		return m.crew_workspace_look_outfit_tie();
	},
};

export const FACE_LABELS: Record<Face, string> = {
	get none() {
		return m.crew_workspace_look_face_none();
	},
	get eyepatch() {
		return m.crew_workspace_look_face_eyepatch();
	},
	get monocle() {
		return m.crew_workspace_look_face_monocle();
	},
	get "round-glasses"() {
		return m.crew_workspace_look_face_glasses();
	},
	get sunglasses() {
		return m.crew_workspace_look_face_sunglasses();
	},
	get goggles() {
		return m.crew_workspace_look_face_goggles();
	},
};

/** One-click presets: dress for a job, keep the crewmate's own colour and eyes. */
export const ROLE_KITS: {
	id: string;
	label: string;
	kit: Pick<CrewmateLook, "hat" | "tool" | "outfit" | "face">;
}[] = [
	{
		id: "captain",
		get label() {
			return m.crew_workspace_look_kit_captain();
		},
		kit: { hat: "bicorne", tool: "compass", outfit: "coat", face: "none" },
	},
	{
		id: "sales",
		get label() {
			return m.crew_workspace_look_kit_sales();
		},
		kit: { hat: "straw", tool: "pin", outfit: "satchel", face: "none" },
	},
	{
		id: "reviewer",
		get label() {
			return m.crew_workspace_look_kit_reviewer();
		},
		kit: { hat: "none", tool: "magnifier", outfit: "tie", face: "round-glasses" },
	},
	{
		id: "lookout",
		get label() {
			return m.crew_workspace_look_kit_lookout();
		},
		kit: { hat: "bandana", tool: "spyglass", outfit: "none", face: "eyepatch" },
	},
	{
		id: "scribe",
		get label() {
			return m.crew_workspace_look_kit_scribe();
		},
		kit: { hat: "beanie", tool: "quill", outfit: "scarf", face: "monocle" },
	},
	{
		id: "keeper",
		get label() {
			return m.crew_workspace_look_kit_keeper();
		},
		kit: { hat: "wizard", tool: "lantern", outfit: "coat", face: "none" },
	},
];
