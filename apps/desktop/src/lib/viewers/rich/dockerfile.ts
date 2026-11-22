import { m } from "#lib/i18n/m.ts";

/** Dockerfile parser: instructions with continuations and heredocs, stages, ports, lint warnings. */

export interface Instruction {
	op: string;
	args: string;
	line: number;
	/** Preceding comment lines. */
	comment: string;
}

export interface Stage {
	index: number;
	/** `AS name`, else null. */
	name: string | null;
	image: string;
	platform: string | null;
	instructions: Instruction[];
	line: number;
}

export interface Warning {
	line: number;
	message: string;
}

export interface DockerDoc {
	stages: Stage[];
	/** ARGs before the first FROM. */
	globalArgs: Instruction[];
	ports: { port: string; stage: number }[];
	warnings: Warning[];
	escape: string;
}

/** Split into instructions. Handles `\` (or the `# escape=` char) continuations, comments inside them, and `<<EOF` heredocs. */
export function parseDockerfile(text: string): { instructions: Instruction[]; escape: string } {
	const raw = text.replace(/\r\n?/g, "\n").split("\n");
	let escape = "\\";
	for (const l of raw) {
		const d = /^#\s*(escape)\s*=\s*(\S)\s*$/i.exec(l);
		if (d) escape = d[2];
		else if (!/^#\s*\w+\s*=/.test(l)) break;
	}
	const out: Instruction[] = [];
	let comment: string[] = [];
	for (let i = 0; i < raw.length; i++) {
		const first = raw[i];
		const t = first.trim();
		if (!t) {
			comment = [];
			continue;
		}
		if (t.startsWith("#")) {
			comment.push(t.replace(/^#\s?/, ""));
			continue;
		}
		const line = i + 1;
		let body = first.trimStart();
		// A continuation keeps going across comment lines (`# …` inside `RUN a \`).
		let continued = body.endsWith(escape);
		body = continued ? body.slice(0, -1).trimEnd() : body;
		while (continued && i + 1 < raw.length) {
			i++;
			const next = raw[i].trim();
			if (next.startsWith("#")) continue;
			continued = next.endsWith(escape);
			body += " " + (continued ? next.slice(0, -1).trimEnd() : next);
		}
		const m = /^(\w+)\s*(.*)$/s.exec(body);
		if (!m) continue;
		let args = m[2];
		// Heredocs: `RUN <<EOF` … `EOF` (also `<<-EOF`, `<<'EOF'`).
		const here = [...args.matchAll(/<<-?\s*(["']?)([A-Za-z_][A-Za-z0-9_]*)\1/g)].map((h) => h[2]);
		for (const tag of here) {
			const lines: string[] = [];
			while (i + 1 < raw.length) {
				i++;
				if (raw[i].trim() === tag) break;
				lines.push(raw[i]);
			}
			args += `\n${lines.join("\n")}\n${tag}`;
		}
		out.push({ op: m[1].toUpperCase(), args, line, comment: comment.join(" ") });
		comment = [];
	}
	return { instructions: out, escape };
}

/** `${NAME}` / `$NAME` / `${NAME:-x}` replaced by the default of a global `ARG NAME=value`, else left as written. */
export function expandArgs(image: string, args: Instruction[]): string {
	const defaults = new Map(
		args.flatMap((a) => {
			const m = /^([A-Za-z_][A-Za-z0-9_]*)=(.*)$/.exec(a.args.trim());
			return m ? [[m[1], m[2].replace(/^["']|["']$/g, "")] as const] : [];
		}),
	);
	return image.replace(
		/\$\{([A-Za-z_][A-Za-z0-9_]*)(?::?-([^}]*))?\}|\$([A-Za-z_][A-Za-z0-9_]*)/g,
		(all, a, fallback, b) => {
			const v = defaults.get(a ?? b) ?? fallback;
			return v === undefined ? all : v;
		},
	);
}

export function analyzeDockerfile(text: string): DockerDoc {
	const { instructions, escape } = parseDockerfile(text);
	const stages: Stage[] = [];
	const globalArgs: Instruction[] = [];
	const ports: DockerDoc["ports"] = [];
	const warnings: Warning[] = [];
	let user: { name: string; line: number } | null = null;
	const flush = () => {
		if (user && /^(root|0)(:|$)/.test(user.name))
			warnings.push({ line: user.line, message: m.viewers_devops_dockerfile_warn_root_user() });
	};
	for (const ins of instructions) {
		if (ins.op === "FROM") {
			if (stages.length) flush();
			user = null;
			const f = /^(?:--platform=(\S+)\s+)?(\S+)(?:\s+AS\s+(\S+))?/i.exec(ins.args);
			const image = expandArgs(f?.[2] ?? ins.args, globalArgs);
			stages.push({
				index: stages.length,
				name: f?.[3] ?? null,
				image,
				platform: f?.[1] ?? null,
				instructions: [],
				line: ins.line,
			});
			const ref = parseImage(image);
			const isStage = stages.slice(0, -1).some((s) => s.name === image);
			if (!isStage && image !== "scratch" && !image.includes("$")) {
				if (!ref.tag && !ref.digest)
					warnings.push({ line: ins.line, message: m.viewers_devops_dockerfile_warn_no_tag({ image }) });
				else if (ref.tag === "latest")
					warnings.push({ line: ins.line, message: m.viewers_devops_dockerfile_warn_latest({ image }) });
			}
			continue;
		}
		if (!stages.length) {
			if (ins.op === "ARG") globalArgs.push(ins);
			continue;
		}
		const st = stages[stages.length - 1];
		st.instructions.push(ins);
		if (ins.op === "EXPOSE")
			for (const p of ins.args.split(/\s+/).filter(Boolean)) ports.push({ port: p, stage: st.index });
		if (ins.op === "USER") user = { name: ins.args.trim(), line: ins.line };
		if (ins.op === "RUN") {
			if (/apt-get\s+install/.test(ins.args) && !/--no-install-recommends/.test(ins.args))
				warnings.push({ line: ins.line, message: m.viewers_devops_dockerfile_warn_apt_recommends() });
			if (
				/apt-get\s+install/.test(ins.args) &&
				!/rm\s+-rf\s+\/var\/lib\/apt\/lists/.test(ins.args) &&
				!/apt-get\s+clean/.test(ins.args)
			)
				warnings.push({ line: ins.line, message: m.viewers_devops_dockerfile_warn_apt_lists() });
			if (/\b(curl|wget)\b[^|]*\|\s*(sudo\s+)?(ba)?sh\b/.test(ins.args))
				warnings.push({ line: ins.line, message: m.viewers_devops_dockerfile_warn_pipe_shell() });
		}
		if (ins.op === "ADD" && !/^\s*(--\S+\s+)*https?:|\.(tar|tgz|gz|xz|bz2)(\s|$)/.test(ins.args))
			warnings.push({ line: ins.line, message: m.viewers_devops_dockerfile_warn_add_local() });
		if (ins.op === "MAINTAINER")
			warnings.push({ line: ins.line, message: m.viewers_devops_dockerfile_warn_maintainer() });
	}
	if (stages.length) {
		if (user === null)
			warnings.push({
				line: stages[stages.length - 1].line,
				message: m.viewers_devops_dockerfile_warn_no_user(),
			});
		else flush();
	}
	return { stages, globalArgs, ports, warnings, escape };
}

export interface ImageRef {
	registry: string | null;
	repo: string;
	tag: string | null;
	digest: string | null;
	/** Browser page for the image on its registry, null when unknown. */
	url: string | null;
}

/** `ghcr.io/o/r:1.2@sha256:…`, `node:20-alpine`, `docker.io/library/node`. */
export function parseImage(image: string): ImageRef {
	let rest = image;
	let digest: string | null = null;
	const at = rest.indexOf("@");
	if (at >= 0) {
		digest = rest.slice(at + 1);
		rest = rest.slice(0, at);
	}
	let tag: string | null = null;
	const colon = rest.lastIndexOf(":");
	if (colon > rest.lastIndexOf("/")) {
		tag = rest.slice(colon + 1);
		rest = rest.slice(0, colon);
	}
	const parts = rest.split("/");
	let registry: string | null = null;
	if (parts.length > 1 && /[.:]|^localhost$/.test(parts[0])) registry = parts.shift()!;
	const repo = parts.join("/");
	let url: string | null = null;
	if (!registry || registry === "docker.io" || registry === "index.docker.io")
		url = repo.includes("/") ? `https://hub.docker.com/r/${repo}` : `https://hub.docker.com/_/${repo}`;
	else if (registry === "ghcr.io")
		url = `https://github.com/${repo.split("/").slice(0, 2).join("/")}/pkgs/container/${repo.split("/").slice(2).join("/") || repo.split("/")[1]}`;
	else if (registry === "quay.io") url = `https://quay.io/repository/${repo}`;
	else if (registry === "gcr.io" || registry.endsWith("-docker.pkg.dev")) url = null;
	return { registry, repo, tag, digest, url };
}
