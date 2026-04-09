import {
	AnimatedTerminal,
	BentoGrid,
	ComparisonTable,
	CopyCommand,
	RuntimeTabs,
	StatsBar,
	StepFlow,
} from "@/components/home";
import { Activity, GitBranch, Globe, Lock, Rocket, Server } from "lucide-react";
import Link from "next/link";

const terminalLines = [
	{ text: "curl -fsSL https://atlas.codeatlas.com.br/install.sh | bash", type: "command" as const },
	{ text: "Atlas installed", type: "success" as const },
	{ text: "", type: "blank" as const },
	{ text: "atlas infra setup --host root@vps", type: "command" as const },
	{ text: "? Runtime: Docker Swarm", type: "output" as const },
	{ text: "Docker installed", type: "success" as const },
	{ text: "Traefik configured", type: "success" as const },
	{ text: "Monitoring ready", type: "success" as const },
	{ text: "", type: "blank" as const },
	{ text: "atlas deploy", type: "command" as const },
	{ text: "Built in 12s", type: "success" as const },
	{ text: "Pushed to GHCR", type: "success" as const },
	{ text: "Live at https://myapp.com", type: "success" as const },
];

const features = [
	{
		icon: Server,
		title: "Choose your runtime",
		description: "K3s or Docker Swarm. Pick during setup, switch anytime with atlas infra migrate.",
		wide: true,
	},
	{
		icon: Globe,
		title: "Automatic DNS & HTTPS",
		description: "Cloudflare DNS + Let's Encrypt. Zero config. Just set your domain.",
	},
	{
		icon: Lock,
		title: "Encrypted secrets",
		description: "AES-256-GCM at rest. Synced to your cluster. Never stored in plain text.",
	},
	{
		icon: GitBranch,
		title: "Preview environments",
		description: "One branch = one URL. Automatic cleanup when you're done.",
	},
	{
		icon: Activity,
		title: "Built-in monitoring",
		description: "Prometheus + Grafana + Loki. Pre-configured. No setup required.",
	},
	{
		icon: Rocket,
		title: "One-command deploy",
		description: "Build, push, deploy, DNS, HTTPS — atlas deploy and you're live.",
		wide: true,
	},
];

const steps = [
	{ number: 1, title: "Install", code: "curl -fsSL .../install.sh | bash", time: "30 seconds" },
	{ number: 2, title: "Setup", code: "atlas infra setup --host root@vps", time: "5 minutes" },
	{ number: 3, title: "Ship", code: "atlas deploy", time: "Done. You're live." },
];

export default function HomePage() {
	return (
		<div className="flex flex-col">
			{/* ── Hero ── */}
			<section className="relative flex flex-col items-center px-4 pb-24 pt-20 text-center">
				<div className="pointer-events-none absolute inset-0 overflow-hidden">
					<div className="absolute left-1/2 top-0 h-[600px] w-[900px] -translate-x-1/2 rounded-full bg-[radial-gradient(ellipse,rgba(129,140,248,0.12),transparent_70%)] blur-3xl" />
				</div>

				<div className="relative">
					<div className="animate-fade-in-up mb-6 inline-flex items-center rounded-full border border-fd-primary/15 bg-fd-primary/5 px-4 py-1.5 text-xs text-fd-primary">
						Open Source · MIT License
					</div>

					<h1 className="animate-fade-in-up animate-delay-100 mb-4 bg-gradient-to-r from-indigo-600 to-sky-600 bg-clip-text font-[family-name:var(--font-mono)] text-5xl font-extrabold tracking-tighter text-transparent dark:from-indigo-400 dark:to-sky-400 sm:text-7xl">
						atlas deploy
					</h1>

					<p className="animate-fade-in-up animate-delay-200 mx-auto mb-8 max-w-lg text-lg text-fd-muted-foreground">
						DNS, HTTPS, secrets, monitoring — on your servers.
						<br />
						One command. No vendor lock-in.
					</p>

					<div className="animate-fade-in-up animate-delay-300 mb-12 flex flex-col items-center gap-3 sm:flex-row sm:justify-center">
						<Link
							href="/docs"
							className="rounded-lg bg-fd-primary px-6 py-2.5 text-sm font-medium text-fd-primary-foreground transition-opacity hover:opacity-90"
						>
							Get Started
						</Link>
						<Link
							href="https://github.com/codeatlasdev/atlas"
							className="rounded-lg border border-fd-border px-6 py-2.5 text-sm font-medium text-fd-muted-foreground transition-colors hover:border-fd-ring/30 hover:text-fd-foreground"
						>
							GitHub
						</Link>
					</div>

					<div className="animate-fade-in-up animate-delay-400 flex justify-center">
						<AnimatedTerminal lines={terminalLines} />
					</div>
				</div>
			</section>

			{/* ── Stats ── */}
			<section className="mx-auto w-full max-w-4xl px-4">
				<StatsBar
					stats={["20+ CLI commands", "K3s + Docker Swarm", "AES-256 encrypted", "MIT License"]}
				/>
			</section>

			{/* ── Code-first ── */}
			<section className="mx-auto w-full max-w-5xl px-4 py-24">
				<div className="grid items-center gap-12 lg:grid-cols-2">
					<div>
						<h2 className="mb-4 text-3xl font-bold tracking-tight text-fd-foreground">
							Define. Deploy. Done.
						</h2>
						<p className="leading-relaxed text-fd-muted-foreground">
							Your entire stack in one file. Atlas reads{" "}
							<code className="rounded bg-fd-primary/10 px-1.5 py-0.5 font-[family-name:var(--font-mono)] text-sm text-fd-primary">
								atlas.yaml
							</code>{" "}
							and handles the rest — build, push, deploy, DNS, HTTPS.
						</p>
					</div>
					<div className="rounded-xl border border-fd-border bg-fd-background p-5 font-[family-name:var(--font-mono)] text-sm leading-relaxed">
						<div className="mb-1 text-fd-muted-foreground/60"># atlas.yaml</div>
						<div>
							<span className="text-fd-primary">name</span>
							<span className="text-fd-muted-foreground">: </span>
							<span className="text-emerald-400">myapp</span>
						</div>
						<div>
							<span className="text-fd-primary">services</span>
							<span className="text-fd-muted-foreground">:</span>
						</div>
						<div className="pl-4">
							<span className="text-fd-primary">api</span>
							<span className="text-fd-muted-foreground">:</span>
						</div>
						<div className="pl-8">
							<span className="text-fd-muted-foreground">type: </span>
							<span className="text-emerald-400">api</span>
						</div>
						<div className="pl-8">
							<span className="text-fd-muted-foreground">port: </span>
							<span className="text-amber-400">3001</span>
						</div>
						<div className="pl-8">
							<span className="text-fd-muted-foreground">domain: </span>
							<span className="text-emerald-400">api.myapp.com</span>
						</div>
						<div className="pl-4">
							<span className="text-fd-primary">web</span>
							<span className="text-fd-muted-foreground">:</span>
						</div>
						<div className="pl-8">
							<span className="text-fd-muted-foreground">type: </span>
							<span className="text-emerald-400">web</span>
						</div>
						<div className="pl-8">
							<span className="text-fd-muted-foreground">domain: </span>
							<span className="text-emerald-400">myapp.com</span>
						</div>
						<div>
							<span className="text-fd-primary">infra</span>
							<span className="text-fd-muted-foreground">:</span>
						</div>
						<div className="pl-4">
							<span className="text-fd-muted-foreground">postgres: </span>
							<span className="text-amber-400">true</span>
						</div>
						<div className="pl-4">
							<span className="text-fd-muted-foreground">redis: </span>
							<span className="text-amber-400">true</span>
						</div>
					</div>
				</div>
			</section>

			{/* ── Features bento ── */}
			<section className="bg-dot-pattern mx-auto w-full max-w-5xl px-4 py-24">
				<h2 className="mb-2 text-center text-3xl font-bold tracking-tight text-fd-foreground">
					Everything you need
				</h2>
				<p className="mb-12 text-center text-fd-muted-foreground">
					No plugins. No add-ons. It&apos;s all built in.
				</p>
				<BentoGrid items={features} />
			</section>

			{/* ── How it works ── */}
			<section className="mx-auto w-full max-w-4xl px-4 py-24">
				<h2 className="mb-2 text-center text-3xl font-bold tracking-tight text-fd-foreground">
					Zero to production
				</h2>
				<p className="mb-12 text-center text-fd-muted-foreground">
					Three commands. That&apos;s it.
				</p>
				<StepFlow steps={steps} />
			</section>

			{/* ── Runtime tabs ── */}
			<section className="mx-auto w-full max-w-3xl px-4 py-24">
				<h2 className="mb-2 text-center text-3xl font-bold tracking-tight text-fd-foreground">
					Pick your runtime
				</h2>
				<p className="mb-12 text-center text-fd-muted-foreground">
					Switch anytime with{" "}
					<code className="rounded bg-fd-primary/10 px-1.5 py-0.5 font-[family-name:var(--font-mono)] text-xs text-fd-primary">
						atlas infra migrate
					</code>
				</p>
				<RuntimeTabs />
			</section>

			{/* ── Comparison ── */}
			<section className="mx-auto w-full max-w-4xl px-4 py-24">
				<h2 className="mb-2 text-center text-3xl font-bold tracking-tight text-fd-foreground">
					Atlas vs. the alternatives
				</h2>
				<p className="mb-12 text-center text-fd-muted-foreground">
					Full stack, self-hosted, no compromises.
				</p>
				<ComparisonTable />
			</section>

			{/* ── CTA footer ── */}
			<section className="mx-auto w-full max-w-2xl px-4 pb-32 pt-12 text-center">
				<p className="mb-6 text-lg text-fd-muted-foreground">
					You scrolled all the way down.
					<br />
					You&apos;re clearly interested.
				</p>
				<div className="mb-6">
					<CopyCommand command="curl -fsSL https://atlas.codeatlas.com.br/install.sh | bash" />
				</div>
				<Link
					href="/docs"
					className="text-sm font-medium text-fd-primary transition-opacity hover:opacity-80"
				>
					Read the docs →
				</Link>
			</section>
		</div>
	);
}
