/** Settings → Agents → Add agent (#56): the form, validated into a `customAgents` entry of settings.json. */
import { parseArgs } from "./args.ts";

export interface CustomAgentForm {
	name: string;
	/** Program, then any fixed arguments, written like a shell command line. */
	command: string;
	args: string;
	/** Arguments that resume a session; `{session}` is replaced by its id. */
	resume: string;
	/** One `KEY=value` per line. */
	env: string;
}

/** What Rust reads (`agent::registry::customs`). */
export interface CustomAgentEntry {
	id: string;
	label: string;
	command: string;
	args: string[];
	resume: string[];
	env: Record<string, string>;
}

type Result = { ok: true; entry: CustomAgentEntry } | { ok: false; error: string };

/** `taken`: every agent id that exists (built-ins and custom), so the new id never shadows one. */
export function customAgentEntry(form: CustomAgentForm, taken: readonly string[]): Result {
	const label = form.name.trim();
	if (!label) return { ok: false, error: "Give the agent a name." };
	const slug = label
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, "-")
		.replace(/^-+|-+$/g, "")
		.slice(0, 28);
	if (!slug) return { ok: false, error: "The name needs at least one letter or digit." };
	let id = slug;
	for (let n = 2; taken.includes(id); n++) id = `${slug}-${n}`;

	const cmd = parseArgs(form.command);
	if (!cmd.ok) return { ok: false, error: `Command: ${cmd.error}` };
	if (!cmd.args[0]) return { ok: false, error: "Enter the command that starts the agent." };
	const args = parseArgs(form.args);
	if (!args.ok) return { ok: false, error: `Arguments: ${args.error}` };
	const resume = parseArgs(form.resume);
	if (!resume.ok) return { ok: false, error: `Resume: ${resume.error}` };

	const env: Record<string, string> = {};
	for (const line of form.env.split("\n")) {
		if (!line.trim()) continue;
		const eq = line.indexOf("=");
		const key = line.slice(0, eq).trim();
		if (eq < 1 || !key) return { ok: false, error: `Environment: "${line.trim()}" is not KEY=value.` };
		if (key.startsWith("ATLAS_"))
			return { ok: false, error: `Environment: ${key} is set by Atlas; ATLAS_ variables are reserved.` };
		env[key] = line.slice(eq + 1).trim();
	}
	return {
		ok: true,
		entry: {
			id,
			label,
			command: cmd.args[0],
			args: [...cmd.args.slice(1), ...args.args],
			resume: resume.args,
			env,
		},
	};
}
