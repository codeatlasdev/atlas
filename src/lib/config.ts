import { homedir } from "node:os"
import { join } from "node:path"
import { mkdir } from "node:fs/promises"

export interface AtlasConfig {
	host?: string
	domain?: string
	githubRepo?: string
	githubToken?: string
	githubUser?: string
	cloudflareToken?: string
	cloudflareAccountId?: string
	tunnelId?: string
	tunnelName?: string
	panelUrl?: string
	panelToken?: string
}

export const ATLAS_DIR = join(homedir(), ".atlas")

const CONFIG_DIR = ATLAS_DIR
const CONFIG_FILE = join(CONFIG_DIR, "config.json")

export async function loadConfig(): Promise<AtlasConfig> {
	try {
		const file = Bun.file(CONFIG_FILE)
		if (await file.exists()) {
			return await file.json()
		}
	} catch {}
	return {}
}

export async function saveConfig(partial: Partial<AtlasConfig>) {
	const current = await loadConfig()
	const merged = { ...current, ...partial }
	await mkdir(CONFIG_DIR, { recursive: true })
	await Bun.write(CONFIG_FILE, JSON.stringify(merged, null, 2))
}
