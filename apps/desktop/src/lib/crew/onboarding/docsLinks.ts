/**
 * Where the crew onboarding points the person for help: ONE map of URLs, every anchor a heading of `docs/crew.md`
 * (checked by `docsLinks.test.ts` against the file itself). The site publishes docs/crew.md at /docs/crew/.
 */
export const CREW_DOCS_URL = "https://atlas.codeatlas.com.br/docs/crew/";

/** Heading slugs of docs/crew.md. */
export const CREW_ANCHORS = {
	install: "install-a-crew-server",
	keyChoice: "add-a-crew-server-in-atlas-desktop",
	sshPath: "one-ssh-path",
	fix: "add-a-crew-server-fails",
	agents: "sign-agents-in",
	crewmates: "crewmates",
} as const;

export type DocsStep = "intro" | "how" | "connect" | "engine" | "ready";

const STEP_ANCHOR: Record<DocsStep, keyof typeof CREW_ANCHORS | null> = {
	intro: null,
	how: null,
	connect: "install",
	engine: "agents",
	ready: "crewmates",
};

const url = (anchor: keyof typeof CREW_ANCHORS | null) =>
	anchor ? `${CREW_DOCS_URL}#${CREW_ANCHORS[anchor]}` : CREW_DOCS_URL;

/** The "Docs" link of a step. */
export const stepDocs = (step: DocsStep) => url(STEP_ANCHOR[step]);

/** The "How to fix" link of a failure: its row in the troubleshooting section. */
export const fixDocs = () => url("fix");

/** The key-or-password explanation, for the sign-in method. */
export const signInDocs = () => url("keyChoice");
