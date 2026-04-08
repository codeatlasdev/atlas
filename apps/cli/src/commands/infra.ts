import { defineCommand } from "citty"

export default defineCommand({
	meta: {
		name: "infra",
		description: "Infrastructure management",
	},
	subCommands: {
		setup: () => import("./infra/setup").then((m) => m.default),
	},
})
