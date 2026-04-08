import { defineCommand } from "citty"
import * as p from "@clack/prompts"
import pc from "picocolors"
import { PanelClient } from "../../lib/panel"

export default defineCommand({
	meta: { name: "server", description: "Manage servers" },
	subCommands: {
		add: () => import("./server-add").then((m) => m.default),
		list: () => import("./server-list").then((m) => m.default),
	},
})
