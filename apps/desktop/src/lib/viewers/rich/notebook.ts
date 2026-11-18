/** Jupyter notebook (nbformat 4) → cells with text/image outputs. */
import { asArray, asRecord, asString } from "#lib/viewers/rich/data.ts";
import { m } from "#lib/i18n/m.ts";

export type Output =
	| { kind: "text"; stream: "stdout" | "stderr" | "result"; text: string }
	| { kind: "image"; mime: string; data: string }
	| { kind: "html"; text: string }
	| { kind: "error"; name: string; value: string; trace: string };

export interface Cell {
	type: "markdown" | "code" | "raw";
	source: string;
	execution: number | null;
	outputs: Output[];
}

export interface Notebook {
	language: string;
	kernel: string;
	cells: Cell[];
	format: string;
}

/** nbformat stores text as `string | string[]`. */
const join = (v: unknown): string => (Array.isArray(v) ? v.join("") : typeof v === "string" ? v : "");

const ANSI = /\x1b\[[0-9;]*m/g;

function outputOf(raw: unknown): Output[] {
	const o = asRecord(raw);
	const type = asString(o.output_type);
	if (type === "stream")
		return [{ kind: "text", stream: o.name === "stderr" ? "stderr" : "stdout", text: join(o.text) }];
	if (type === "error")
		return [
			{
				kind: "error",
				name: asString(o.ename) ?? m.viewers_files_notebook_view_error(),
				value: asString(o.evalue) ?? "",
				trace: asArray(o.traceback).join("\n").replace(ANSI, ""),
			},
		];
	if (type === "execute_result" || type === "display_data") {
		const data = asRecord(o.data);
		for (const mime of ["image/png", "image/jpeg", "image/gif"])
			if (data[mime]) return [{ kind: "image", mime, data: join(data[mime]).replace(/\s/g, "") }];
		if (data["image/svg+xml"]) return [{ kind: "html", text: join(data["image/svg+xml"]) }];
		if (data["text/html"]) return [{ kind: "html", text: join(data["text/html"]) }];
		if (data["text/plain"]) return [{ kind: "text", stream: "result", text: join(data["text/plain"]) }];
	}
	return [];
}

export function readNotebook(value: unknown): Notebook {
	const r = asRecord(value);
	const meta = asRecord(r.metadata);
	const lang =
		asString(asRecord(meta.language_info).name) ?? asString(asRecord(meta.kernelspec).language) ?? "python";
	const cells = asArray(r.cells).map((c): Cell => {
		const cell = asRecord(c);
		const t = cell.cell_type;
		return {
			type: t === "markdown" || t === "code" ? t : "raw",
			source: join(cell.source),
			execution: typeof cell.execution_count === "number" ? cell.execution_count : null,
			outputs: asArray(cell.outputs).flatMap(outputOf),
		};
	});
	return {
		language: lang,
		kernel: asString(asRecord(meta.kernelspec).display_name) ?? lang,
		cells,
		format: `nbformat ${String(r.nbformat ?? "?")}.${String(r.nbformat_minor ?? 0)}`,
	};
}
