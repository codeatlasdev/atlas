// The guard's one decision function: a tool call in, allow or deny out. A denial names the exact fix.
// Rules (docs/contributing.md "Agent guards"): write only inside your own worktree; never push to main or release/*,
// force-push, merge a PR, `git stash`, or delete branches and tags. A maintainer lifts one rule for a whole session
// with `ATLAS_MAINTAINER_OK=<action>` in the environment the session was launched with; commands cannot set it.
import { basename, join, relative, sep } from "node:path";
import { type Ctx, OVERRIDE_ENV } from "./context";
import { inside } from "./paths";
import { parseShell, type Segment } from "./shell";

/** What `ATLAS_MAINTAINER_OK` can lift (comma list, or `all`). */
export type Action = "write" | "push" | "force-push" | "merge" | "stash" | "delete" | "config";

export type Call =
	| { kind: "write"; paths: string[]; cwd?: string }
	| { kind: "bash"; command: string; cwd?: string };

export type Verdict = { allow: true } | { allow: false; action: Action | "self-override"; message: string };

const ALLOW: Verdict = { allow: true };
const MERGE_REFUSED = "refused: agents never merge a pull request (no `gh pr merge`, no `bun run pr:merge`, no API merge, no auto-merge). Leave it open and tell the maintainer it is ready; a maintainer merges it. `bun run pr:merge <N> --dry-run` is fine: it only prints what it would do.";

function deny(ctx: Ctx, action: Action, message: string): Verdict | undefined {
	if (ctx.override.has("all") || ctx.override.has(action)) return undefined;
	return { allow: false, action, message: `${message}\nMaintainer override, set before the session starts (never by the agent): ${OVERRIDE_ENV}=${action}` };
}

// ── file writes ────────────────────────────────────────────────────────────────────────────────────────────────

/** Why the guard refuses a write to the real path `abs`, or `undefined` when it is fine. */
function writeBlock(ctx: Ctx, abs: string): string | undefined {
	const t = ctx.tree();
	if (!t) return undefined;
	if (t.primary) {
		if (!t.denyPrimary || !inside(t.root, abs)) return undefined;
		return `refused: ${abs} is in the main checkout ${t.root}, which is kept free of agent edits (git config atlas.agentWrites=deny). Work in your own worktree: git worktree add -b <type/short-desc> <dir> origin/main, then use absolute paths under <dir>.`;
	}
	if (inside(t.root, abs)) {
		if (relative(t.root, abs).split(sep)[0] !== ".git") return undefined;
		return `refused: ${abs} is git's own data for your worktree (hooks, config, HEAD); change it with git commands, not by editing files.`;
	}
	const other = t.all.find((w) => w !== t.root && inside(w, abs));
	if (other) {
		const kind = other === t.main ? "the main checkout" : "another worktree";
		return `refused: ${abs} is in ${kind} ${other}, outside your worktree ${t.root}; use the absolute path under your worktree: ${join(t.root, relative(other, abs))}`;
	}
	if (ctx.scratch.some((d) => inside(d, abs))) return undefined;
	return `refused: ${abs} is outside your worktree ${t.root}; use the absolute path under your worktree (or the temp dir for scratch files).`;
}

function writeVerdict(ctx: Ctx, raw: string, base: string): Verdict | undefined {
	if (/[$`]/.test(raw) || raw.startsWith("/dev/") || raw === "NUL") return undefined; // cannot be resolved statically
	const glob = raw.search(/[*?[{]/);
	const msg = writeBlock(ctx, ctx.physical(glob < 0 ? raw : raw.slice(0, glob) || ".", base));
	return msg ? deny(ctx, "write", msg) : undefined;
}

// ── shell commands ─────────────────────────────────────────────────────────────────────────────────────────────

const WRAPPERS: Record<string, true> = { env: true, command: true, exec: true, nohup: true, time: true, builtin: true, sudo: true, doas: true, caffeinate: true };

/** The command proper, without env assignments, `sudo`, `nice`, `timeout`, `atlas run --heavy --`… in front of it. */
function unwrap(words: string[]): string[] {
	let w = [...words];
	for (let n = 0; n < 8 && w.length; n++) {
		while (w.length && /^[A-Za-z_][A-Za-z0-9_]*=/.test(w[0])) w.shift();
		const c = cmdName(w[0]);
		if (Object.hasOwn(WRAPPERS, c)) {
			w.shift();
			while (w[0]?.startsWith("-")) {
				const f = w.shift() as string;
				if ((c === "sudo" && /^-[ugCp]$/.test(f)) || (c === "env" && f === "-u")) w.shift();
			}
		} else if (c === "nice") {
			w.shift();
			if (w[0] === "-n") w.splice(0, 2);
			else if (/^-\d+$/.test(w[0] ?? "")) w.shift();
		} else if (c === "timeout") {
			w.shift();
			while (w[0]?.startsWith("-")) w.shift();
			w.shift();
		} else if (c === "atlas" && w[1] === "run") {
			const k = w.indexOf("--");
			if (k < 0) break;
			w = w.slice(k + 1);
		} else if (c === "heavy.sh") w.shift();
		else break;
	}
	return w;
}

const cmdName = (w: string | undefined): string => basename(w ?? "").replace(/\.exe$/i, "");

/** Git subcommands that change the files of the checkout they run in. */
const MUTATES_TREE: Record<string, true> = Object.fromEntries(
	["add", "rm", "mv", "commit", "checkout", "switch", "restore", "reset", "merge", "rebase", "pull", "cherry-pick", "revert", "apply", "am", "clean", "stash"].map((s) => [s, true as const]),
);

function pushVerdict(ctx: Ctx, args: string[], dir: string): Verdict | undefined {
	const pos: string[] = [];
	let force = false;
	let del = false;
	for (let i = 0; i < args.length; i++) {
		const a = args[i];
		if (a === "--no-verify") {
			const v = deny(ctx, "push", "refused: git push --no-verify skips the repo's pre-push guard; remove the flag.");
			if (v) return v;
		} else if (a === "--all" || a === "--mirror") {
			const v = deny(ctx, "push", `refused: git push ${a} pushes main as well; push your branch by name (git push -u origin <branch>).`);
			if (v) return v;
		} else if (a === "--delete" || a === "--prune") del = true;
		else if (a === "--force" || a.startsWith("--force-with-lease") || a === "--force-if-includes") force = true;
		else if (/^-[a-zA-Z]+$/.test(a)) {
			if (a.includes("f")) force = true;
			if (a.includes("d")) del = true;
		} else if (["--repo", "-o", "--push-option", "--receive-pack", "--exec"].includes(a)) i++;
		else if (!a.startsWith("-")) pos.push(a);
	}
	const refspecs = pos.slice(1);
	const cur = ctx.branch(dir);
	const dests: string[] = [];
	if (refspecs.length === 0) dests.push(cur ?? "");
	for (let spec of refspecs) {
		if (spec.startsWith("+")) {
			force = true;
			spec = spec.slice(1);
		}
		const [src, dst] = spec.includes(":") ? (spec.split(":", 2) as [string, string]) : [spec, spec];
		if (src === "") del = true;
		const dest = (dst || src) === "HEAD" ? (cur ?? "") : dst || src;
		dests.push(dest.replace(/^refs\/heads\//, ""));
	}
	for (const dest of dests) {
		if (/^(main|master)$/.test(dest) || dest.startsWith("release/") || dest.includes("*")) {
			const v = deny(ctx, "push", `refused: git push to ${dest || "the current branch"}. main and release/* change only through a reviewed pull request: push your branch (git push -u origin <type/short-desc>) and open a draft PR (gh pr create --draft).`);
			if (v) return v;
		}
	}
	if (force) {
		const v = deny(ctx, "force-push", "refused: force-push rewrites remote history and can destroy other people's commits. Push a new commit instead (git push); if a rebase really is needed, ask the maintainer.");
		if (v) return v;
	}
	if (del) return deny(ctx, "delete", "refused: deleting remote branches or tags is a maintainer action; leave the branch, GitHub removes it when the PR merges.");
	return undefined;
}

function gitVerdict(ctx: Ctx, args: string[], cwd: string): Verdict | undefined {
	let dir = cwd;
	let i = 0;
	for (; i < args.length; i++) {
		const a = args[i];
		if (a === "-C") dir = ctx.physical(args[++i] ?? ".", dir);
		else if (a === "-c") {
			if (/^core\.hookspath=/i.test(args[++i] ?? "")) {
				const v = deny(ctx, "push", "refused: git -c core.hooksPath=… switches off the repo's git hooks (the pre-push guard); remove it.");
				if (v) return v;
			}
		} else if (a === "--work-tree") dir = ctx.physical(args[++i] ?? ".", dir);
		else if (a.startsWith("--work-tree=")) dir = ctx.physical(a.slice("--work-tree=".length), dir);
		else if (["--git-dir", "--namespace", "--super-prefix", "--exec-path"].includes(a)) i++;
		else if (!a.startsWith("-")) break;
	}
	const sub = args[i];
	const rest = args.slice(i + 1);
	if (sub !== undefined && Object.hasOwn(MUTATES_TREE, sub) && !(sub === "stash" && ["list", "show"].includes(rest[0]))) {
		const msg = writeBlock(ctx, dir);
		if (msg) {
			const v = deny(ctx, "write", `${msg.replace(/^refused: [^ ]+ /, `refused: git ${sub} would change files in ${dir}, which `)}`);
			if (v) return v;
		}
	}
	switch (sub) {
		case "stash":
			if (["list", "show"].includes(rest[0])) return undefined;
			return deny(ctx, "stash", "refused: git stash is shared by every worktree of this repository (one refs/stash), so another agent's `git stash pop` applies your work. Commit a WIP commit on your branch, or save a patch: git diff > /tmp/x.patch, and apply it with git apply.");
		case "push":
			return pushVerdict(ctx, rest, dir);
		case "branch":
			if (rest.some((a) => a === "--delete" || /^-[a-zA-Z]*[dD][a-zA-Z]*$/.test(a))) return deny(ctx, "delete", "refused: deleting branches is a maintainer action; leave it, `git worktree remove` and the PR merge clean up.");
			return undefined;
		case "tag":
			if (rest.some((a) => a === "--delete" || a === "--force" || /^-[a-zA-Z]*[dfD][a-zA-Z]*$/.test(a))) return deny(ctx, "delete", "refused: deleting or moving a tag is a maintainer action (v* tags are immutable); create a new tag instead.");
			return undefined;
		case "update-ref":
			return rest.includes("-d") ? deny(ctx, "delete", "refused: git update-ref -d deletes a ref; that is a maintainer action.") : undefined;
		case "config": {
			const read = rest.some((a) => ["--get", "--get-all", "--get-regexp", "--list", "-l"].includes(a));
			const key = rest.find((a) => !a.startsWith("-"))?.toLowerCase();
			if (!read && (key === "core.hookspath" || key === "atlas.agentwrites")) return deny(ctx, "config", `refused: ${key} configures the agent guards; only a maintainer changes it.`);
			return undefined;
		}
		default:
			return undefined;
	}
}

function ghVerdict(ctx: Ctx, args: string[]): Verdict | undefined {
	const pos = args.filter((a) => !a.startsWith("-"));
	const merge = () => deny(ctx, "merge", MERGE_REFUSED);
	if (pos[0] === "pr" && pos[1] === "merge") return merge();
	if (pos[0] === "api") {
		const text = args.join(" ");
		if (/pulls\/\d+\/merge|mergePullRequest|enablePullRequestAutoMerge/.test(text)) return merge();
		if (/(-X|--method)[ =]?DELETE/i.test(text) && /git\/refs\//.test(text)) return deny(ctx, "delete", "refused: deleting branches or tags through the API is a maintainer action.");
	}
	return undefined;
}

/** Commands whose arguments are files they create, change or delete. `last`: only the destination. */
const WRITERS: Record<string, "all" | "last"> = { rm: "all", rmdir: "all", unlink: "all", touch: "all", mkdir: "all", tee: "all", truncate: "all", mv: "all", cp: "last", install: "last", ln: "last", rsync: "last" };
const VALUE_FLAGS = new Set(["-m", "-s", "-t", "-d", "-r", "-o", "-g"]);

function writerPaths(cmd: string, args: string[]): string[] {
	const paths: string[] = [];
	let target: string | undefined;
	let afterDashes = false;
	for (let i = 0; i < args.length; i++) {
		const a = args[i];
		if (!afterDashes && a === "--") afterDashes = true;
		else if (!afterDashes && (a === "-t" || a === "--target-directory") && WRITERS[cmd] === "last") target = args[++i] ?? "";
		else if (!afterDashes && a.startsWith("--target-directory=")) target = a.slice("--target-directory=".length);
		else if (!afterDashes && a.startsWith("-")) {
			if (VALUE_FLAGS.has(a) && cmd !== "cp" && cmd !== "mv" && cmd !== "rm") i++;
		} else paths.push(a);
	}
	if (target !== undefined) return [target];
	const local = paths.filter((p) => p && !/^[^/\\]+:[^\\/]/.test(p)); // `host:path` is remote
	return WRITERS[cmd] === "last" ? local.slice(-1) : local;
}

interface State {
	cwd: string;
}

function walk(ctx: Ctx, segments: Segment[], state: State): Verdict | undefined {
	for (const seg of segments) {
		for (const target of seg.writes) {
			const v = writeVerdict(ctx, target, state.cwd);
			if (v) return v;
		}
		const words = unwrap(seg.words);
		if (!words.length) continue;
		const cmd = cmdName(words[0]);
		const args = words.slice(1);
		let v: Verdict | undefined;
		// `bun run pr:merge` runs `gh pr merge` in a subprocess the command text never shows; only its --dry-run is harmless.
		if (/^(bun|bunx|npm|npx|pnpm|yarn|node|tsx)$/.test(cmd) && /(^|\s)pr:merge(\s|$)|pr-merge\.ts/.test(args.join(" ")) && !args.includes("--dry-run")) {
			v = deny(ctx, "merge", MERGE_REFUSED);
		} else if (cmd === "cd" || cmd === "pushd") v = cdVerdict(ctx, args, state);
		else if (cmd === "git") v = gitVerdict(ctx, args, state.cwd);
		else if (cmd === "gh") v = ghVerdict(ctx, args);
		else if (["bash", "sh", "zsh", "dash"].includes(cmd)) {
			const c = args.findIndex((a) => /^-[a-z]*c[a-z]*$/.test(a));
			if (c >= 0 && args[c + 1] !== undefined) v = walk(ctx, parseShell(args[c + 1]), { ...state });
		} else if (cmd === "eval") v = walk(ctx, parseShell(args.join(" ")), { ...state });
		else if (cmd === "dd") {
			for (const a of args) if (a.startsWith("of=") && (v = writeVerdict(ctx, a.slice(3), state.cwd))) break;
		} else if (Object.hasOwn(WRITERS, cmd)) {
			for (const p of writerPaths(cmd, args)) if ((v = writeVerdict(ctx, p, state.cwd))) break;
		}
		if (v) return v;
	}
	return undefined;
}

function cdVerdict(ctx: Ctx, args: string[], state: State): Verdict | undefined {
	const target = args.find((a) => !a.startsWith("-")) ?? ctx.home;
	if (target === "-" || /[$`]/.test(target)) return undefined;
	const abs = ctx.physical(target, state.cwd);
	state.cwd = abs;
	const t = ctx.tree();
	if (!t || t.primary || inside(t.root, abs)) return undefined;
	const other = t.all.find((w) => inside(w, abs));
	if (!other) return undefined;
	const kind = other === t.main ? "the main checkout" : "another worktree";
	return deny(ctx, "write", `refused: cd into ${kind} ${other} leaves your worktree ${t.root}, and everything after it would run there. Stay in ${t.root} (git -C ${t.root} …, absolute paths under it).`);
}

// ── entry point ────────────────────────────────────────────────────────────────────────────────────────────────

export function decide(call: Call, ctx: Ctx): Verdict {
	if (call.kind === "write") {
		for (const p of call.paths) {
			const v = writeVerdict(ctx, p, call.cwd ? ctx.physical(call.cwd) : ctx.cwd);
			if (v) return v;
		}
		return ALLOW;
	}
	if (call.command.includes(OVERRIDE_ENV)) {
		return { allow: false, action: "self-override", message: `refused: the command mentions ${OVERRIDE_ENV}. Only a maintainer sets it, in the environment the session was launched with. Remove it; if you need that action, ask the maintainer to run it or to relaunch the session with the override.` };
	}
	const base = call.cwd ? ctx.physical(call.cwd) : ctx.cwd;
	return walk(ctx, parseShell(call.command), { cwd: base }) ?? ALLOW;
}
