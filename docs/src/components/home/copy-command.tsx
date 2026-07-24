"use client";

import { Check, Copy } from "lucide-react";
import { useState } from "react";

export function CopyCommand({ command }: { command: string }) {
	const [copied, setCopied] = useState(false);

	function copy() {
		navigator.clipboard.writeText(command);
		setCopied(true);
		setTimeout(() => setCopied(false), 2000);
	}

	return (
		<div className="flex items-center gap-3 rounded-lg border border-fd-border bg-fd-background px-4 py-3 font-[family-name:var(--font-mono)] text-sm">
			<span className="text-fd-muted-foreground">$</span>
			<span className="flex-1 text-fd-foreground">{command}</span>
			<button
				type="button"
				onClick={copy}
				className="text-fd-muted-foreground transition-colors hover:text-fd-primary"
			>
				{copied ? <Check className="size-4 text-emerald-400" /> : <Copy className="size-4" />}
			</button>
		</div>
	);
}
