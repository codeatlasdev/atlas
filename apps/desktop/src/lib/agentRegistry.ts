/**
 * The built-in coding agents (omp, Claude Code, Codex, OpenCode…): ids, labels, binaries, hook setup names and
 * capabilities. The one source is `crates/atlas-core/data/agents.json`, which Rust embeds
 * (`atlas_core::agents`) and Atlas Mobile reads as generated Dart; this module imports that same file, so a
 * list of agents is never typed again in the frontend. Add an agent there, not here.
 */
import registry from "../../../../crates/atlas-core/data/agents.json";

/** One entry of `agents.json`; the shape Rust serializes (`atlas_core::agents::AgentDef`). */
export interface BuiltinAgent {
	id: string;
	label: string;
	/** The program name on `PATH`; also what a crew server's sign-in terminal runs. */
	bin: string;
	/** `atlas setup <setup>` installs the hooks that report the agent's state. */
	setup: string;
	yolo_args: string[];
	/** Resume argv; `{session}` is the session id (omp: its session file). */
	resume: string[];
	prompt_flag: string | null;
	/** How it answers one prompt and exits with no tools (AI commit messages); null = not offered there. */
	one_shot: string[] | null;
	sessions_dir: string | null;
	install: string;
	/** The environment variable the CLI reads its API key from. */
	key_env: string;
	/** Whether a headless crewmate can pause a tool call for approval. */
	can_ask: boolean;
}

/** Every built-in agent, in display order. */
export const BUILTIN_AGENTS: readonly BuiltinAgent[] = registry;

/** The built-in agent `id`, or `undefined` (a custom agent, or none). */
export function builtinAgent(id: string | undefined): BuiltinAgent | undefined {
	return BUILTIN_AGENTS.find((a) => a.id === id);
}

export function isBuiltinAgent(id: string | undefined): boolean {
	return builtinAgent(id) !== undefined;
}

/** Whether agent `id` can write a commit message (has a one-shot run): the Git setting only offers these. */
export function canWriteCommits(id: string | undefined): boolean {
	return builtinAgent(id)?.one_shot != null;
}

/** The shell line that resumes `session` of agent `id` (what the sessions list copies). */
export function resumeCommand(id: string, session: string): string {
	const a = builtinAgent(id);
	const argv = a ? a.resume.map((t) => t.replace("{session}", session)) : ["--resume", session];
	return [a?.bin ?? id, ...argv].join(" ");
}
