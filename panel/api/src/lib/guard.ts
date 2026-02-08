const JWT_SECRET = process.env.JWT_SECRET ?? "atlas-dev-secret-change-in-production"

export interface AuthContext {
	userId: number
	orgId: number
	role: "admin" | "dev" | "viewer"
	username: string
}

const encoder = new TextEncoder()
const keyData = encoder.encode(JWT_SECRET)
const key = await crypto.subtle.importKey(
	"raw",
	keyData.buffer as ArrayBuffer,
	{ name: "HMAC", hash: "SHA-256" },
	false,
	["sign", "verify"],
)

function base64url(data: Uint8Array): string {
	return btoa(String.fromCharCode(...data))
		.replace(/\+/g, "-")
		.replace(/\//g, "_")
		.replace(/=+$/, "")
}

function base64urlDecode(str: string): Uint8Array {
	const padded = str.replace(/-/g, "+").replace(/_/g, "/")
	const binary = atob(padded)
	return Uint8Array.from(binary, (c) => c.charCodeAt(0))
}

export async function signToken(payload: Record<string, unknown>): Promise<string> {
	const header = base64url(encoder.encode(JSON.stringify({ alg: "HS256", typ: "JWT" })))
	const exp = Math.floor(Date.now() / 1000) + 30 * 24 * 60 * 60
	const body = base64url(encoder.encode(JSON.stringify({ ...payload, exp })))
	const data = encoder.encode(`${header}.${body}`)
	const signature = new Uint8Array(await crypto.subtle.sign("HMAC", key, data.buffer as ArrayBuffer))
	return `${header}.${body}.${base64url(signature)}`
}

export async function verifyToken(token: string): Promise<AuthContext | null> {
	try {
		const [header, body, sig] = token.split(".")
		if (!header || !body || !sig) return null

		const data = encoder.encode(`${header}.${body}`)
		const signature = base64urlDecode(sig)
		const valid = await crypto.subtle.verify("HMAC", key, signature.buffer as ArrayBuffer, data.buffer as ArrayBuffer)
		if (!valid) return null

		const payload = JSON.parse(new TextDecoder().decode(base64urlDecode(body)))
		if (payload.exp && payload.exp < Date.now() / 1000) return null

		return {
			userId: Number(payload.sub),
			orgId: Number(payload.org),
			role: payload.role,
			username: payload.username,
		}
	} catch {
		return null
	}
}

export async function requireAuth(authorization: string | undefined): Promise<AuthContext> {
	const token = authorization?.replace("Bearer ", "")
	if (!token) throw new Error("Unauthorized")
	const auth = await verifyToken(token)
	if (!auth) throw new Error("Invalid token")
	return auth
}

export function assertRole(auth: AuthContext, ...roles: AuthContext["role"][]): void {
	if (!roles.includes(auth.role)) {
		throw new Error(`Requires role: ${roles.join(" or ")}`)
	}
}
