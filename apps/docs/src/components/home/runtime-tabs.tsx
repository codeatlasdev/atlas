"use client";

import { useState } from "react";

const runtimes = {
	swarm: {
		label: "Docker Swarm",
		description: "The lightweight option. Built into Docker — no extra binaries, minimal overhead.",
		bestFor: "Small teams, 1–2 GB RAM servers",
		overhead: "~100 MB",
		features: [
			"Traefik v3 with automatic HTTPS",
			"Prometheus + Grafana + Loki",
			"Docker Compose native",
			"Multi-node with overlay networking",
			"Preview environments",
		],
	},
	k3s: {
		label: "K3s (Kubernetes)",
		description: "The full-featured option. Lightweight Kubernetes with the entire K8s ecosystem.",
		bestFor: "Larger teams, 2 GB+ RAM servers",
		overhead: "~512 MB",
		features: [
			"Everything Swarm has, plus:",
			"cert-manager for TLS automation",
			"ArgoCD for GitOps",
			"HPA for auto-scaling",
			"Full kubectl access",
			"Helm chart support",
		],
	},
} as const;

type Runtime = keyof typeof runtimes;

export function RuntimeTabs() {
	const [active, setActive] = useState<Runtime>("swarm");
	const rt = runtimes[active];

	return (
		<div>
			<div className="mb-6 flex gap-1 rounded-lg border border-fd-border bg-fd-background p-1">
				{(Object.keys(runtimes) as Runtime[]).map((key) => (
					<button
						key={key}
						type="button"
						onClick={() => setActive(key)}
						className={`flex-1 rounded-md px-4 py-2 text-sm font-medium transition-all ${
							active === key
								? "bg-fd-muted text-fd-primary shadow-sm"
								: "text-fd-muted-foreground hover:text-fd-foreground"
						}`}
					>
						{runtimes[key].label}
					</button>
				))}
			</div>
			<div className="rounded-xl border border-fd-border bg-fd-card p-6">
				<p className="mb-4 text-fd-foreground">{rt.description}</p>
				<div className="mb-4 flex flex-wrap gap-4 text-sm">
					<span className="text-fd-muted-foreground">
						Best for: <span className="text-fd-foreground/80">{rt.bestFor}</span>
					</span>
					<span className="text-fd-muted-foreground">
						RAM: <span className="text-fd-foreground/80">{rt.overhead}</span>
					</span>
				</div>
				<ul className="space-y-2">
					{rt.features.map((f) => (
						<li key={f} className="flex items-start gap-2 text-sm text-fd-foreground/80">
							<span className="mt-0.5 text-emerald-400">✓</span>
							{f}
						</li>
					))}
				</ul>
			</div>
		</div>
	);
}
