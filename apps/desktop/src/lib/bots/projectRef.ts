/**
 * What a bot on a ship gets as its project when the user picks one of their Atlas projects.
 *
 * A ship cannot see this Mac's disk, so a local project only travels as its git origin: the ship clones it (shared
 * clone + a worktree per bot, crates/atlas-host/src/bots/project.rs). Sending the Mac's path instead made the agent
 * start in a folder that does not exist on the ship and exit with code 1.
 */
export type BotProjectRef =
	| { kind: "git"; name: string; url: string; branch?: string }
	| { kind: "path"; name: string; path: string; branch?: string };

export type Resolved = { ok: true; project: BotProjectRef } | { ok: false; error: string };

export function projectForShip(
	local: { name: string; path: string; branch?: string },
	origin: string | null,
	shipKey: string,
): Resolved {
	const branch = local.branch || undefined;
	// The local hostd runs on this Mac: the path is real there.
	if (shipKey === "")
		return { ok: true, project: { kind: "path", name: local.name, path: local.path, branch } };
	if (!origin)
		return {
			ok: false,
			error: `${local.name} has no git remote, so the ship cannot get it. Push it to GitHub (git remote add origin …), then pick it again.`,
		};
	return { ok: true, project: { kind: "git", name: local.name, url: origin, branch } };
}
