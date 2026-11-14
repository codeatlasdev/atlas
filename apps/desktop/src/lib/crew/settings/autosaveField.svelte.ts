/**
 * One autosaved field of the crewmate settings dialog: its status line ("Saving…" / "Saved" / the error) and the
 * run-a-save wrapper that drives it. Every field (role, approval, short replies, memory) is one of these.
 */
import { errText } from "#lib/api.ts";

export type SaveStatus = "idle" | "saving" | "saved" | "error";

const SAVED_FLASH_MS = 1500;

export class AutosaveField {
	status = $state<SaveStatus>("idle");
	error = $state("");

	reset() {
		this.status = "idle";
		this.error = "";
	}

	/** Run `save`; resolves true when it went through (status flashes "Saved"), false when it failed (status shows the error). */
	async run(save: () => Promise<void>): Promise<boolean> {
		this.status = "saving";
		this.error = "";
		try {
			await save();
			this.status = "saved";
			setTimeout(() => (this.status = "idle"), SAVED_FLASH_MS);
			return true;
		} catch (e) {
			this.error = errText(e);
			this.status = "error";
			return false;
		}
	}
}
