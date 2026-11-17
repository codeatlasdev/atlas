// Path helpers for the agent guards: resolve a path the way the kernel would (symlinks before `..`), and test containment.
import { lstatSync, readlinkSync, realpathSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, isAbsolute, join, parse, relative, sep } from "node:path";

/** `~` and `~/x` to the home directory. */
export function expandHome(p: string, home: string): string {
	if (p === "~") return home;
	if (p.startsWith("~/") || p.startsWith("~\\")) return join(home, p.slice(2));
	return p;
}

/**
 * Git Bash and Cygwin write Windows paths as `/c/Users/x`, `/cygdrive/c/x` and `/tmp/x`; Windows APIs read those as
 * "current drive, folder c". On Windows they become `C:\Users\x` and the temp dir; elsewhere `p` is returned as is.
 */
export function fromMsys(p: string, win = process.platform === "win32", tmp = tmpdir()): string {
	if (!win) return p;
	const drive = p.match(/^(?:\/cygdrive)?\/([a-zA-Z])(?:\/(.*))?$/);
	if (drive) return `${drive[1].toUpperCase()}:\\${(drive[2] ?? "").replaceAll("/", "\\")}`;
	const t = p.match(/^\/tmp(?:\/(.*))?$/);
	return t ? join(tmp, t[1] ?? "") : p;
}

/** The canonical spelling of an existing path: Windows 8.3 names (`RUNNER~1`) and letter case are what the OS says. */
function canon(p: string): string {
	try {
		return realpathSync.native(p);
	} catch {
		return p;
	}
}

const parts = (p: string) => p.split(/[\\/]+/).filter((c) => c && c !== ".");

/**
 * Where `p` (relative to `cwd`) really lands. Walks it component by component, following every symlink on the way, so
 * `link/..` goes to the link target's parent as the OS does (a lexical `..` would not), and a dangling symlink resolves
 * to its target (a write through it creates the target). The tail that does not exist yet is appended as given.
 */
export function physical(p: string, cwd: string, home: string): string {
	const expanded = fromMsys(expandHome(p, home));
	let cur = isAbsolute(expanded) ? parse(expanded).root : physical(isAbsolute(cwd) ? cwd : process.cwd(), "/", home);
	const todo = parts(isAbsolute(expanded) ? expanded.slice(cur.length) : expanded);
	for (let hops = 0; todo.length && hops < 255; hops++) {
		const c = todo.shift() as string;
		if (c === "..") {
			cur = dirname(cur);
			continue;
		}
		const next = join(cur, c);
		let link: string | undefined;
		try {
			if (lstatSync(next).isSymbolicLink()) link = readlinkSync(next).replace(/^\\\\\?\\/, "");
		} catch {
			// Does not exist, so neither does anything below it: the rest is appended.
			return todo.reduce((out, d) => (d === ".." ? dirname(out) : join(out, d)), join(canon(cur), c));
		}
		if (link === undefined) cur = next;
		else {
			if (isAbsolute(link)) cur = parse(link).root;
			todo.unshift(...parts(isAbsolute(link) ? link.slice(parse(link).root.length) : link));
		}
	}
	return canon(cur);
}

/** `child` is `parent` or lies below it, by path components (`..x` is a name, not a parent). */
export function inside(parent: string, child: string): boolean {
	const r = relative(parent, child);
	if (r === "") return true;
	if (isAbsolute(r)) return false;
	return r.split(sep)[0] !== "..";
}
