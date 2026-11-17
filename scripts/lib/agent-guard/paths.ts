// Path helpers for the agent guards: resolve a path the way the kernel would (symlinks before `..`), and test containment.
import { lstatSync, readlinkSync } from "node:fs";
import { dirname, isAbsolute, join, parse, relative, sep } from "node:path";

/** `~` and `~/x` to the home directory. */
export function expandHome(p: string, home: string): string {
	if (p === "~") return home;
	if (p.startsWith("~/") || p.startsWith("~\\")) return join(home, p.slice(2));
	return p;
}

const parts = (p: string) => p.split(/[\\/]+/).filter((c) => c && c !== ".");

/**
 * Where `p` (relative to `cwd`) really lands. Walks it component by component, following every symlink on the way, so
 * `link/..` goes to the link target's parent as the OS does (a lexical `..` would not), and a dangling symlink resolves
 * to its target (a write through it creates the target). The tail that does not exist yet is appended as given.
 */
export function physical(p: string, cwd: string, home: string): string {
	const expanded = expandHome(p, home);
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
			return todo.reduce((out, d) => (d === ".." ? dirname(out) : join(out, d)), next);
		}
		if (link === undefined) cur = next;
		else {
			if (isAbsolute(link)) cur = parse(link).root;
			todo.unshift(...parts(isAbsolute(link) ? link.slice(parse(link).root.length) : link));
		}
	}
	return cur;
}

/** `child` is `parent` or lies below it, by path components (`..x` is a name, not a parent). */
export function inside(parent: string, child: string): boolean {
	const r = relative(parent, child);
	if (r === "") return true;
	if (isAbsolute(r)) return false;
	return r.split(sep)[0] !== "..";
}
