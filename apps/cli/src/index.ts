#!/usr/bin/env bun
import { defineCommand, runMain } from "citty"
import { version } from "../package.json"

const main = defineCommand({
	meta: {
		name: "atlas",
		version,
		description: "CodeAtlas Internal Developer Platform",
	},
	subCommands: {
		login: () => import("./commands/login").then((m) => m.default),
		whoami: () => import("./commands/whoami").then((m) => m.default),
		create: () => import("./commands/create").then((m) => m.default),
		init: () => import("./commands/init").then((m) => m.default),
		infra: () => import("./commands/infra").then((m) => m.default),
		deploy: () => import("./commands/deploy").then((m) => m.default),
		preview: () => import("./commands/preview").then((m) => m.default),
		tunnel: () => import("./commands/tunnel").then((m) => m.default),
		status: () => import("./commands/status").then((m) => m.default),
		logs: () => import("./commands/logs").then((m) => m.default),
		exec: () => import("./commands/exec").then((m) => m.default),
		restart: () => import("./commands/restart").then((m) => m.default),
		scale: () => import("./commands/scale").then((m) => m.default),
		env: () => import("./commands/env").then((m) => m.default),
		db: () => import("./commands/db").then((m) => m.default),
		open: () => import("./commands/open").then((m) => m.default),
		panel: () => import("./commands/panel").then((m) => m.default),
	},
})

runMain(main)
