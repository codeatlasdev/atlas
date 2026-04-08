import { defineCommand } from "citty"

export default defineCommand({
	meta: { name: "panel", description: "Control Panel management" },
	subCommands: {
		setup: () => import("./setup").then((m) => m.default),
		status: () => import("./status").then((m) => m.default),
		config: () => import("./config").then((m) => m.default),
		server: () => import("./server").then((m) => m.default),
	},
})
