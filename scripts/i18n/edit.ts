// Small writers over the source tree for the commands that change it (add, mv): read an area file as an object, set or
// remove a key, write it back in canonical form. Files that do not exist yet read as `{}`.
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { MESSAGES_DIR } from "./types.ts";
import { canonicalJson, detectIndent, readText } from "./util.ts";

export const areaPath = (area: string, file: string) => `${MESSAGES_DIR}/${area}/${file}.json`;

export function readObject(root: string, rel: string): Record<string, unknown> {
	const p = join(root, rel);
	return existsSync(p) ? JSON.parse(readText(p)) : {};
}

export function writeObject(root: string, rel: string, obj: Record<string, unknown>) {
	const p = join(root, rel);
	const indent = existsSync(p) ? detectIndent(readText(p)) : "\t";
	mkdirSync(dirname(p), { recursive: true });
	writeFileSync(p, canonicalJson(obj, indent));
}

export function setKey(root: string, area: string, file: string, key: string, value: unknown) {
	const rel = areaPath(area, file);
	writeObject(root, rel, { ...readObject(root, rel), [key]: value });
}

/** Remove `key` from an area file; the removed value, or undefined when it was not there. */
export function removeKey(root: string, area: string, file: string, key: string): unknown {
	const rel = areaPath(area, file);
	if (!existsSync(join(root, rel))) return undefined;
	const obj = readObject(root, rel);
	const value = obj[key];
	if (key in obj) {
		delete obj[key];
		writeObject(root, rel, obj);
	}
	return value;
}

/** A failure of a command: the message names the command that fixes it. Exit code 1. */
export class CommandError extends Error {}
