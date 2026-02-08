const ENCRYPTION_KEY = process.env.ENCRYPTION_KEY ?? process.env.JWT_SECRET ?? "atlas-dev-secret-change-in-production"

const encoder = new TextEncoder()
const decoder = new TextDecoder()

async function deriveKey(): Promise<CryptoKey> {
	const raw = await crypto.subtle.digest("SHA-256", encoder.encode(ENCRYPTION_KEY))
	return crypto.subtle.importKey("raw", raw, "AES-GCM", false, ["encrypt", "decrypt"])
}

const keyPromise = deriveKey()

export async function encrypt(plaintext: string): Promise<string> {
	const key = await keyPromise
	const iv = crypto.getRandomValues(new Uint8Array(12))
	const ciphertext = new Uint8Array(
		await crypto.subtle.encrypt({ name: "AES-GCM", iv }, key, encoder.encode(plaintext)),
	)
	// iv:ciphertext as base64
	const combined = new Uint8Array(iv.length + ciphertext.length)
	combined.set(iv)
	combined.set(ciphertext, iv.length)
	return btoa(String.fromCharCode(...combined))
}

export async function decrypt(encoded: string): Promise<string> {
	const key = await keyPromise
	const combined = Uint8Array.from(atob(encoded), (c) => c.charCodeAt(0))
	const iv = combined.slice(0, 12)
	const ciphertext = combined.slice(12)
	const plaintext = await crypto.subtle.decrypt(
		{ name: "AES-GCM", iv },
		key,
		ciphertext.buffer.slice(ciphertext.byteOffset, ciphertext.byteOffset + ciphertext.byteLength),
	)
	return decoder.decode(plaintext)
}
