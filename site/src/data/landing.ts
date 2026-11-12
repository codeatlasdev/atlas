// Content and build-time data of the landing page (pages/index.astro keeps only the markup).
import { mediaVersion } from "./media";
import { SITE } from "./site";

/** `/media/<f>?v=<version>`; the version comes from the tracked manifest (see media.ts). */
export const mediaUrl = (f: string) => {
	const v = mediaVersion(f);
	return v ? `/media/${f}?v=${v}` : `/media/${f}`;
};

export const FAQ = [
	{ q: "How do I install it?", a: "On macOS and Linux run `curl -fsSL https://atlas.codeatlas.com.br/install.sh | sh`. On Windows, in PowerShell, run `irm https://atlas.codeatlas.com.br/install.ps1 | iex`. Either one downloads the latest release, verifies its SHA-256 and installs `atlas` into `~/.atlas/bin` (`%USERPROFILE%\\.atlas\\bin` on Windows). Then `atlas desktop` installs the desktop app. Prefer a download? Get the .dmg for macOS, `Atlas-Setup.exe` for Windows (one click, per user, no admin rights) or a .deb, .rpm, Arch package or AppImage for Linux from the download page." },
	{ q: "What does it run on?", a: "macOS 13 or later on Apple silicon, Windows 10 (1809+) and 11 on x64, and Linux x86_64 (glibc 2.35+), each with the CLI, the TUI, the MCP server and the desktop app. Coming soon: Intel Macs, Windows on Arm (the x64 build runs emulated today) and Linux on Arm; ask to be notified on the download page. Two things are macOS only for now: the native GPU terminal (Windows and Linux use xterm.js terminals) and capture/recording plus agent test instances." },
	{ q: "Is the Windows installer signed?", a: "Not yet. `Atlas-Setup.exe` is built by GitHub Actions from the release tag and has a published SHA-256, but it has no code signature yet, so Windows SmartScreen may say \"Windows protected your PC\". Click More info, then Run anyway. Signing through SignPath Foundation is pending the Foundation's approval; until the first signed release the Windows files are unsigned." },
	{ q: "Is it open source?", a: "Yes. The source is on GitHub under MIT or Apache-2.0, at your option. Issues and pull requests are welcome." },
	{ q: "Do I need the desktop app to use atlas.yaml?", a: "No. The CLI, the TUI and the MCP server work on their own. The desktop app is a client of the same runners, so the app, a terminal and an agent all see the same live state." },
	{ q: "Which agents does it work with?", a: "omp, Claude Code and Codex. `atlas setup` detects them, shows what it would change, and with `--apply` adds the MCP server, the skills and the status hooks. Cursor and other MCP clients can be configured by hand." },
	{ q: "Does quitting the app stop my stack?", a: "No. Start runs `atlas dev --detach` and terminals live in a per-user daemon, so stacks and agents keep running when the app quits, crashes or updates." },
	{ q: "Does it send my code or logs anywhere?", a: "No. Atlas is local first. Logs stay in `.logs/` in your project and the control socket is local. The release-notes email form on this page is the only thing on the site that talks to a server; the app only contacts GitHub to check for updates." },
	{ q: "How is it different from process-compose, mprocs or Tilt?", a: "Those run processes. Atlas also gives you the workspace around them: a worktree per agent, a project panel with live charts, a GPU terminal on macOS, and an MCP server so agents can read logs and restart services. The README's comparison table is honest about where the others win, such as replicas." },
];

/** FAQ answers are markdown-ish: `code` spans become <code>. */
export const codeSpans = (s: string) => s.replace(/`([^`]+)`/g, "<code>$1</code>");

export const JSON_LD = {
	"@context": "https://schema.org",
	"@graph": [
		{
			"@type": "SoftwareApplication",
			name: "Atlas",
			applicationCategory: "DeveloperApplication",
			operatingSystem: ["macOS 13+ (Apple silicon)", "Windows 10 (1809+) and 11 (x64)", "Linux x86_64 (glibc 2.35+)"],
			description: SITE.description,
			url: SITE.url,
			license: ["https://opensource.org/licenses/MIT", "https://opensource.org/licenses/Apache-2.0"],
			publisher: { "@type": "Organization", name: "CodeAtlas", url: SITE.codeatlas },
		},
		{
			"@type": "FAQPage",
			mainEntity: FAQ.map((f) => ({ "@type": "Question", name: f.q, acceptedAnswer: { "@type": "Answer", text: f.a.replace(/`/g, "") } })),
		},
	],
};

const LOG_ROWS = [
	'22:12:22 worker job email.send done in 37ms',
	'22:12:23 api {"level":30,"msg":"GET /orders","ms":17,"status":200}',
	'22:12:23 web [hmr] updated src/routes/cart.tsx',
	'22:12:24 api {"level":30,"msg":"POST /checkout","ms":221,"status":201}',
	'22:12:24 worker WARN queue lag 1.2s',
	'22:12:25 search query "running shoes" 14ms',
	'22:12:26 database checkpoint complete: wrote 42 buffers',
	'22:12:26 api {"level":30,"msg":"GET /orders","ms":90,"status":200}',
	'22:12:27 web [hmr] updated src/routes/cart.tsx',
	'22:12:28 api {"level":50,"msg":"stripe timeout","ms":5000,"status":504}',
];
/** The scrolling log river wraps around, so the rows appear twice. */
export const LOG_RIVER = [...LOG_ROWS, ...LOG_ROWS].join("\n");
