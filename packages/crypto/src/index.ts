const encoder = new TextEncoder()
const decoder = new TextDecoder()

async function deriveKey(secret: string): Promise<CryptoKey> {
	const raw = await crypto.subtle.digest("SHA-256", encoder.encode(secret))
	return crypto.subtle.importKey("raw", raw, "AES-GCM", false, ["encrypt", "decrypt"])
}

export async function encryptWithKey(plaintext: string, secret: string): Promise<string> {
	const key = await deriveKey(secret)
	const iv = crypto.getRandomValues(new Uint8Array(12))
	const ciphertext = new Uint8Array(
		await crypto.subtle.encrypt({ name: "AES-GCM", iv }, key, encoder.encode(plaintext)),
	)
	const combined = new Uint8Array(iv.length + ciphertext.length)
	combined.set(iv)
	combined.set(ciphertext, iv.length)
	return btoa(String.fromCharCode(...combined))
}

export async function decryptWithKey(encoded: string, secret: string): Promise<string> {
	const key = await deriveKey(secret)
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

// Convenience wrappers — use ENCRYPTION_KEY from env
function getSecret(): string {
	const key = process.env.ENCRYPTION_KEY
	if (!key) throw new Error("ENCRYPTION_KEY environment variable is required")
	return key
}

export const encrypt = (plaintext: string) => encryptWithKey(plaintext, getSecret())
export const decrypt = (encoded: string) => decryptWithKey(encoded, getSecret())
