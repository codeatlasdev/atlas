import { defineCommand } from "citty"
import pc from "picocolors"
import { loadConfig } from "../lib/config"

export default defineCommand({
	meta: { name: "whoami", description: "Show current authentication status" },
	async run() {
		const config = await loadConfig()

		console.log(pc.bold("\n🔑 Atlas Config\n"))
		console.log(`  User:    ${config.githubUser ? pc.green(config.githubUser) : pc.dim("not logged in")}`)
		console.log(`  Repo:    ${config.githubRepo || pc.dim("not set")}`)
		console.log(`  Host:    ${config.host || pc.dim("not set")}`)
		console.log(`  Domain:  ${config.domain || pc.dim("not set")}`)
		console.log(`  Token:   ${config.githubToken ? pc.green("configured") : pc.dim("not set")}`)
		console.log()
	},
})
