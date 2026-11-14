/**
 * What a crewmate on a crew server gets as its project when the user picks one of their Atlas projects.
 *
 * A crew server cannot see this Mac's disk, so a local project only travels as its git origin: the crew server clones it (shared
 * clone + a worktree per crewmate, crates/atlas-host/src/crew/project.rs). Sending the Mac's path instead made the agent
 * start in a folder that does not exist on the crew server and exit with code 1.
 */
export type CrewmateProjectRef =
	| { kind: "git"; name: string; url: string; branch?: string }
	| { kind: "path"; name: string; path: string; branch?: string };

export type Resolved = { ok: true; project: CrewmateProjectRef } | { ok: false; error: string };

export function projectForServer(
	local: { name: string; path: string; branch?: string },
	origin: string | null,
	serverKey: string,
): Resolved {
	const branch = local.branch || undefined;
	// The local hostd runs on this Mac: the path is real there.
	if (serverKey === "")
		return { ok: true, project: { kind: "path", name: local.name, path: local.path, branch } };
	if (!origin)
		return {
			ok: false,
			error: `${local.name} has no git remote, so the crew server cannot get it. Push it to GitHub (git remote add origin …), then pick it again.`,
		};
	return { ok: true, project: { kind: "git", name: local.name, url: origin, branch } };
}
