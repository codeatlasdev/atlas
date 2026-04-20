/**
 * Shared types for the Atlas infrastructure layer.
 *
 * These live in @atlas/api to break the circular dependency between
 * @atlas/runtime, @atlas/provisioner, and @atlas/firecracker.
 */

import type { SSHResult } from "@atlas/ssh";

// ── Runtime ──

export type RuntimeType = "k3s" | "swarm" | "firecracker";

export interface RuntimeService {
	readonly type: RuntimeType;

	deploy(stack: string, service: string, image: string): Promise<boolean>;
	rolloutStatus(stack: string, service: string, timeoutSec?: number): Promise<boolean>;
	scale(stack: string, service: string, replicas: number): Promise<boolean>;
	getPods(stack: string): Promise<string>;
	streamLogs(
		stack: string,
		service: string,
		opts?: { tail?: number; follow?: boolean },
	): Promise<ReadableStream<Uint8Array>>;
	exec(stack: string, service: string, command: string): Promise<SSHResult>;
	syncSecrets(stack: string, name: string, data: Record<string, string>): Promise<boolean>;
	deleteSecretKey(stack: string, name: string, key: string): Promise<boolean>;
	applyManifest(stack: string, manifest: string): Promise<SSHResult>;
	deleteResource(stack: string, resource: string, name: string): Promise<boolean>;
	runJob(stack: string, name: string, image: string, envFrom?: string): Promise<boolean>;
}

// ── Provisioner ──

export interface ProvisionPhase {
	name: string;
	script: string;
}
