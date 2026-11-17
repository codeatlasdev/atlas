// Hashes: the 12-hex lock stamp of an English value, and the git blob sha that tracks a docs source.
import { createHash } from "node:crypto";

/** 12 hex chars of the sha256 of `text`: what a lock file stores per key. */
export const hash12 = (text: string) => createHash("sha256").update(text).digest("hex").slice(0, 12);

/** What `git hash-object <file>` prints: sha1 of `blob <bytes>\0<content>`. CRLF is folded to LF first, which is
 * what git stores under `core.autocrlf`, so a Windows checkout agrees with the blob in the repository. */
export function gitBlobSha(content: string | Uint8Array): string {
	const text = typeof content === "string" ? content : Buffer.from(content).toString("utf8");
	const body = Buffer.from(text.replace(/\r\n/g, "\n"), "utf8");
	return createHash("sha1").update(`blob ${body.length}\0`).update(body).digest("hex");
}
