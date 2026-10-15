/** Base64 <-> bytes/text for the host protocol (pure: no Tauri, so tests that stub `host.ts` still decode). */

const enc = new TextEncoder();
const dec = new TextDecoder();

export function b64encode(s: string | Uint8Array): string {
	const bytes = typeof s === "string" ? enc.encode(s) : s;
	let bin = "";
	for (let i = 0; i < bytes.length; i += 0x8000) bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
	return btoa(bin);
}

export function b64bytes(b: string): Uint8Array {
	const bin = atob(b);
	const out = new Uint8Array(bin.length);
	for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
	return out;
}

export const b64decode = (b: string) => dec.decode(b64bytes(b));
