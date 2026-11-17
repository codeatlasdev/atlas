// i18n/glossary.yaml: what parses, what is rejected, how terms match, and that the shipped file is valid.
import { describe, expect, test } from "bun:test";
import { join } from "node:path";
import { loadGlossary, osLine, parseGlossary, ptPtHits, termRegex, textIssues } from "./glossary.ts";

const loc = (g: string, source: string, target: string, description?: string) => textIssues({ glossary: parseGlossary(g), locale: "pt-BR", source, target, description });

describe("parseGlossary", () => {
	test.each([
		["no term", "- dnt: true\n", "non-empty `term`"],
		["a duplicate term (any case)", "- term: Atlas\n  dnt: true\n- term: atlas\n  dnt: true\n", "duplicate"],
		["an entry that says nothing", "- term: x\n", "needs `dnt: true`"],
		["an empty rendering", "- term: x\n  pt-BR: ''\n", "non-empty string"],
		["an os entry missing a platform", "- term: x\n  os:\n    en: { mac: a, windows: b }\n", "linux"],
		["a non-list document", "term: x\n", "list"],
	])("rejects %s", (_n, yaml, text) => {
		expect(() => parseGlossary(yaml)).toThrow(text);
	});

	test("accepts dnt, renderings, gender, advice and os forms; an empty file is an empty glossary", () => {
		const g = parseGlossary(`- term: branch
  dnt: true
  gender: { pt-BR: f }
- term: run
  advice: { pt-BR: 'Button: Executar' }
- term: your computer
  os:
    en: { mac: your Mac, windows: this PC, linux: this computer }
    pt-BR: { mac: seu Mac, windows: este PC, linux: este computador }
`);
		expect(g.entries.map((e) => e.term)).toEqual(["branch", "run", "your computer"]);
		expect(osLine(g.entries[2].os?.["pt-BR"] ?? { mac: "", windows: "", linux: "" })).toBe("mac: seu Mac | windows: este PC | linux: este computador");
		expect(parseGlossary("# only a comment\n").entries).toEqual([]);
	});

	test("the shipped glossary.yaml parses and holds the maintainer's decisions", () => {
		const g = loadGlossary(join(import.meta.dir, "..", ".."));
		const get = (t: string) => g.entries.find((e) => e.term === t);
		for (const t of ["branch", "worktree", "issue"]) expect(get(t)?.gender?.["pt-BR"]).toBe("f");
		expect(get("crewmate")?.dnt).toBe(true);
		expect(get("crew server")?.dnt).toBe(true);
		expect(get("run")?.advice?.["pt-BR"]).toContain("Executar");
		expect(get("your computer")?.os?.en).toEqual({ mac: "your Mac", windows: "this PC", linux: "this computer" });
		expect(get("your computer")?.os?.["pt-BR"]).toEqual({ mac: "seu Mac", windows: "este PC", linux: "este computador" });
		expect(get("file manager")?.os?.["pt-BR"]).toEqual({ mac: "Finder", windows: "Explorador de Arquivos", linux: "gerenciador de arquivos" });
		expect(get("trash")?.os?.["pt-BR"]).toEqual({ mac: "Lixeira", windows: "Lixeira", linux: "Lixeira" });
		expect(get("system settings")?.os?.["pt-BR"]).toEqual({ mac: "Ajustes do Sistema", windows: "Configurações", linux: "configurações do sistema" });
		expect(get("alt key")?.os?.["pt-BR"]).toEqual({ mac: "Option", windows: "Alt", linux: "Alt" });
		expect(get("enter key")?.os?.["pt-BR"]).toEqual({ mac: "Return", windows: "Enter", linux: "Enter" });
		expect(get("menu bar")?.os?.["pt-BR"].mac).toBe("a barra de menus");
		expect(get("menu bar")?.os?.["pt-BR"].windows).toBe("a bandeja");
	});
});

describe("term matching", () => {
	test("whole words only, with an optional plural s, and accents count as letters", () => {
		const re = termRegex("log");
		expect(re.test("Open the log")).toBe(true);
		expect(re.test("Show logs")).toBe(true);
		expect(re.test("Login")).toBe(false);
		expect(re.test("catalog")).toBe(false);
		expect(termRegex("serviço").test("serviços")).toBe(true);
		expect(termRegex("ação").test("reação")).toBe(false);
	});

	test("a capitalised term is case-sensitive, a lowercase one is not, a phrase spans spaces", () => {
		expect(termRegex("Crew").test("the crew is here")).toBe(false);
		expect(termRegex("Crew").test("Crew members")).toBe(true);
		expect(termRegex("commit").test("Commit changes")).toBe(true);
		expect(termRegex("crew server").test("Install a crew\n server")).toBe(true);
	});
});

describe("textIssues", () => {
	const G = "- term: crew server\n  dnt: true\n- term: issue\n  dnt: true\n- term: settings\n  pt-BR: configurações\n";

	test("crew server and issue stay English in pt-BR", () => {
		expect(loc(G, "Install a crew server", "Instalar um servidor de crew").map((i) => i.rule)).toEqual(["dnt"]);
		expect(loc(G, "Install a crew server", "Instalar um crew server")).toEqual([]);
		expect(loc(G, "Open the issue", "Abrir o problema").map((i) => i.rule)).toEqual(["dnt"]);
		expect(loc(G, "Open the issue", "Abrir a issue")).toEqual([]);
	});

	test("a rendering mismatch is a warning; a waiver in the description silences it", () => {
		expect(loc(G, "Open settings", "Abrir ajustes")).toEqual([expect.objectContaining({ rule: "glossary", severity: "warn" })]);
		expect(loc(G, "Open settings", "Abrir ajustes", "i18n-allow glossary:settings")).toEqual([]);
	});

	test("pt-PT hits only apply to pt-BR", () => {
		expect(ptPtHits("Guardar o ficheiro").map((h) => h.use).sort()).toEqual(["arquivo", "salvar"]);
		expect(textIssues({ glossary: { entries: [] }, locale: "pt-PT", source: "Save", target: "Guardar" })).toEqual([]);
	});
});

describe("pt-PT deny list", () => {
	test.each([
		["Abrir o ecrã", "tela"],
		["Vários ecrãs", "tela"],
		["O utilizador", "usuário"],
		["Os utilizadores", "usuário"],
		["Um ficheiro", "arquivo"],
		["Dois ficheiros", "arquivo"],
		["As definições", "configurações"],
		["O registo", "registro"],
		["Os registos", "registro"],
		["Partilhar", "compartilhar"],
		["Partilhado com", "compartilhar"],
		["A palavra-passe", "senha"],
		["As palavras-passe", "senha"],
		["Aceder à pasta", "acessar"],
		["Guardar", "salvar"],
		["Guardado", "salvar"],
		["Eliminar", "excluir"],
		["Eliminada", "excluir"],
		["O rato", "mouse"],
		["Transferir", "baixar"],
		["Estou a ler", "gerúndio"],
		["Está a fazer", "gerúndio"],
		["Estão a gravar", "gerúndio"],
	])("%p is pt-PT (use %p)", (text, use) => {
		const hits = ptPtHits(text);
		expect(hits).toHaveLength(1);
		expect(hits[0].use).toContain(use);
	});

	test.each([
		"Abrir a tela",
		"O usuário",
		"Um arquivo",
		"As configurações",
		"O registro",
		"Compartilhar",
		"A senha",
		"Acessar a pasta",
		"Salvar",
		"Excluir",
		"O mouse",
		"Baixar",
		"Está gravando",
		"Onde está a cor",
	])("%p is not flagged", (text) => {
		expect(ptPtHits(text).map((h) => h.word)).toEqual([]);
	});
});
