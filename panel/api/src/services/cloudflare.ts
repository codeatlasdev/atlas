const CF_API = "https://api.cloudflare.com/client/v4"

interface CFResponse<T> {
	success: boolean
	result: T
	errors: { message: string }[]
}

interface Zone {
	id: string
	name: string
}

interface DNSRecord {
	id: string
	type: string
	name: string
	content: string
}

export class CloudflareService {
	constructor(
		private token: string,
		private accountId: string,
	) {}

	private async req<T>(path: string, init?: RequestInit): Promise<T> {
		const res = await fetch(`${CF_API}${path}`, {
			...init,
			headers: {
				Authorization: `Bearer ${this.token}`,
				"Content-Type": "application/json",
				...init?.headers,
			},
		})
		const data = (await res.json()) as CFResponse<T>
		if (!data.success) {
			throw new Error(`Cloudflare: ${data.errors.map((e) => e.message).join(", ")}`)
		}
		return data.result
	}

	async getZone(domain: string): Promise<Zone | null> {
		// Extract root domain (e.g., "api.example.com" → "example.com")
		const parts = domain.split(".")
		const root = parts.slice(-2).join(".")
		const zones = await this.req<Zone[]>(`/zones?name=${root}&per_page=1`)
		return zones[0] ?? null
	}

	async ensureDNS(hostname: string, target: string, type: "A" | "CNAME" = "A"): Promise<{ action: "created" | "updated" | "exists"; recordId: string }> {
		const zone = await this.getZone(hostname)
		if (!zone) throw new Error(`No Cloudflare zone found for ${hostname}`)

		const existing = await this.req<DNSRecord[]>(`/zones/${zone.id}/dns_records?name=${hostname}&type=${type}`)

		if (existing.length > 0) {
			const record = existing[0]
			if (record.content === target) {
				return { action: "exists", recordId: record.id }
			}
			// Update existing record
			const updated = await this.req<DNSRecord>(`/zones/${zone.id}/dns_records/${record.id}`, {
				method: "PATCH",
				body: JSON.stringify({ content: target, proxied: true }),
			})
			return { action: "updated", recordId: updated.id }
		}

		// Create new record
		const created = await this.req<DNSRecord>(`/zones/${zone.id}/dns_records`, {
			method: "POST",
			body: JSON.stringify({ type, name: hostname, content: target, proxied: true }),
		})
		return { action: "created", recordId: created.id }
	}

	async deleteDNS(zoneId: string, recordId: string): Promise<void> {
		await this.req(`/zones/${zoneId}/dns_records/${recordId}`, { method: "DELETE" })
	}

	async verify(): Promise<boolean> {
		try {
			// Test actual DNS access by listing zones
			const zones = await this.req<{ id: string }[]>("/zones?per_page=1")
			return zones.length > 0
		} catch {
			return false
		}
	}
}
