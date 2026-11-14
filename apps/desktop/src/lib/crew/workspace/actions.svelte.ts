/**
 * Crew workspace actions: recruit a new crewmate (on the first reachable crew server, reusing its engine) and delete
 * one. `recruit.creating` is the in-flight flag the buttons disable on.
 */
import { toast } from "@codeatlasdev/ui";
import { errText } from "#lib/api.ts";
import { randomLook } from "#lib/crew/look.ts";
import { replayCrewOnboarding } from "#lib/crew/crewOnboarding.svelte.ts";
import { isCrewServerKey } from "#lib/crew/sshTarget.ts";
import { crew } from "#lib/stores/crew.svelte.ts";

class Recruit {
	creating = $state(false);

	async start() {
		if (this.creating) return;
		this.creating = true;
		const pending = crew.pendingProjectLink;
		try {
			// The crew lives on a crew server (hosts[0] is this Mac, whose hostd has no crewmate runner), and hostd requires an
			// engine: reuse the crew server's crewmates' one. No crew server or no engine yet → the onboarding sets both up.
			const server =
				crew.servers.find((h) => isCrewServerKey(h.key) && h.ok) ??
				crew.servers.find((h) => isCrewServerKey(h.key));
			const agent = server?.crewmates.find((b) => b.agent)?.agent ?? "";
			if (!server || !agent) {
				replayCrewOnboarding();
				return;
			}
			const hostKey = server.key;
			const id = await crew.createCrewmate(hostKey, {
				name: pending ? `${pending.name} Crewmate` : "New crewmate",
				role: "",
				agent,
				approval: "ask",
				look: randomLook(),
				setup_step: "name",
			});
			if (pending) {
				await crew.updateCrewmate(hostKey, id, { project: pending });
				crew.pendingProjectLink = null;
			}
			await crew.select(hostKey, id);
		} catch (e) {
			toast.error(errText(e));
		} finally {
			this.creating = false;
		}
	}
}

export const recruit = new Recruit();

/** Row context menu: "profile" opens the crewmate, "delete" asks first. */
export function handleContextAction(hostKey: string, crewmateId: string, action: "profile" | "delete") {
	if (action === "delete") {
		if (confirm("Delete this crewmate?")) void crew.deleteCrewmate(hostKey, crewmateId);
	} else {
		void crew.select(hostKey, crewmateId);
	}
}
