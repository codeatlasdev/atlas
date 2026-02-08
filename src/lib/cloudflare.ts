const API = "https://api.cloudflare.com/client/v4"

interface CFResponse<T> {
	success: boolean
	errors: { message: string }[]
	result: T
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
	proxied: boolean
}

interface Tunnel {
	id: string
	name: string
	status: string
	token?: string
	credentials_file?: {
		AccountTag: string
		TunnelID: string
		TunnelSecret: string
	}
}

export class CloudflareClient {
	constructor(
		private token: string,
		private accountId: string,
	) {}

	private async req<T>(path: string, init?: RequestInit): Promise<T> {
		const res = await fetch(`${API}${path}`, {
			...init,
			headers: {
				Authorization: `Bearer ${this.token}`,
				"Content-Type": "application/json",
				...init?.headers,
			},
		})
		const data = (await res.json()) as CFResponse<T>
		if (!data.success) {
			throw new Error(`Cloudflare API: ${data.errors.map((e) => e.message).join(", ")}`)
		}
		return data.result
	}

	// ── Zones ──

	async getZoneByName(name: string): Promise<Zone | null> {
		const zones = await this.req<Zone[]>(`/zones?name=${name}&per_page=1`)
		return zones[0] || null
	}

	// ── DNS Records ──

	async listDNSRecords(zoneId: string, name?: string): Promise<DNSRecord[]> {
		const q = name ? `?name=${name}` : ""
		return this.req<DNSRecord[]>(`/zones/${zoneId}/dns_records${q}`)
	}

	async createDNSRecord(
		zoneId: string,
		record: { type: string; name: string; content: string; proxied?: boolean },
	): Promise<DNSRecord> {
		return this.req<DNSRecord>(`/zones/${zoneId}/dns_records`, {
			method: "POST",
			body: JSON.stringify({ proxied: true, ...record }),
		})
	}

	async deleteDNSRecord(zoneId: string, recordId: string): Promise<void> {
		await this.req(`/zones/${zoneId}/dns_records/${recordId}`, { method: "DELETE" })
	}

	// ── Tunnels ──

	async createTunnel(name: string): Promise<Tunnel> {
		return this.req<Tunnel>(`/accounts/${this.accountId}/cfd_tunnel`, {
			method: "POST",
			body: JSON.stringify({ name, config_src: "cloudflare" }),
		})
	}

	async getTunnel(tunnelId: string): Promise<Tunnel> {
		return this.req<Tunnel>(`/accounts/${this.accountId}/cfd_tunnel/${tunnelId}`)
	}

	async getTunnelToken(tunnelId: string): Promise<string> {
		return this.req<string>(`/accounts/${this.accountId}/cfd_tunnel/${tunnelId}/token`)
	}

	async updateTunnelIngress(
		tunnelId: string,
		ingress: { hostname: string; service: string; originRequest?: Record<string, unknown> }[],
	): Promise<void> {
		await this.req(`/accounts/${this.accountId}/cfd_tunnel/${tunnelId}/configurations`, {
			method: "PUT",
			body: JSON.stringify({
				config: {
					ingress: [...ingress, { service: "http_status:404" }],
				},
			}),
		})
	}

	async deleteTunnel(tunnelId: string): Promise<void> {
		await this.req(`/accounts/${this.accountId}/cfd_tunnel/${tunnelId}`, { method: "DELETE" })
	}

	// ── Verify Token ──

	async verify(): Promise<boolean> {
		try {
			const res = await fetch(`${API}/user/tokens/verify`, {
				headers: { Authorization: `Bearer ${this.token}` },
			})
			const data = (await res.json()) as CFResponse<{ status: string }>
			return data.success && data.result.status === "active"
		} catch {
			return false
		}
	}

	// ── Helpers ──

	/** Ensure CNAME exists for hostname → tunnel. Creates if missing. */
	async ensureTunnelDNS(zoneId: string, hostname: string, tunnelId: string): Promise<"created" | "exists"> {
		const existing = await this.listDNSRecords(zoneId, hostname)
		const cname = `${tunnelId}.cfargotunnel.com`

		const match = existing.find((r) => r.type === "CNAME" && r.content === cname)
		if (match) return "exists"

		// Delete stale records for this hostname
		for (const r of existing.filter((r) => r.name === hostname)) {
			await this.deleteDNSRecord(zoneId, r.id)
		}

		await this.createDNSRecord(zoneId, { type: "CNAME", name: hostname, content: cname, proxied: true })
		return "created"
	}
}
