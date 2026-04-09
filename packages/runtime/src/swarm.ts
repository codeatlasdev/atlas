import { ssh } from "@atlas/ssh";
import type { RuntimeService } from "./index";

export class SwarmRuntime implements RuntimeService {
	readonly type = "swarm" as const;

	constructor(private host: string) {}

	private async run(command: string) {
		return ssh(this.host, command);
	}

	private svc(stack: string, service: string) {
		return `${stack}_${service}`;
	}

	async deploy(stack: string, service: string, image: string) {
		const { ok } = await this.run(
			`docker service update --image ${image} ${this.svc(stack, service)}`,
		);
		return ok;
	}

	async rolloutStatus(stack: string, service: string, timeoutSec = 120) {
		// Poll until service converges or timeout
		const { ok } = await this.run(
			`timeout ${timeoutSec} bash -c 'while docker service ps ${this.svc(stack, service)} --format "{{.CurrentState}}" | grep -q "Running"; do
				DESIRED=$(docker service inspect --format "{{.Spec.Mode.Replicated.Replicas}}" ${this.svc(stack, service)} 2>/dev/null || echo 0)
				RUNNING=$(docker service ps ${this.svc(stack, service)} --filter "desired-state=running" --format "{{.CurrentState}}" | grep -c "Running" || echo 0)
				[ "$RUNNING" -ge "$DESIRED" ] && exit 0
				sleep 2
			done'`,
		);
		return ok;
	}

	async scale(stack: string, service: string, replicas: number) {
		const { ok } = await this.run(`docker service scale ${this.svc(stack, service)}=${replicas}`);
		return ok;
	}

	async getPods(stack: string) {
		const { stdout } = await this.run(
			`docker service ls --filter "label=com.docker.stack.namespace=${stack}" --format "table {{.ID}}\t{{.Name}}\t{{.Replicas}}\t{{.Image}}\t{{.Ports}}"`,
		);
		return stdout;
	}

	async streamLogs(
		stack: string,
		service: string,
		opts: { tail?: number; follow?: boolean } = {},
	): Promise<ReadableStream<Uint8Array>> {
		const tail = opts.tail ?? 100;
		const followFlag = opts.follow ? "-f" : "";
		const cmd = `docker service logs ${this.svc(stack, service)} --tail ${tail} ${followFlag} 2>&1`;

		const proc = Bun.spawn(
			["ssh", "-o", "StrictHostKeyChecking=accept-new", "-o", "ConnectTimeout=10", this.host, cmd],
			{ stdout: "pipe", stderr: "pipe" },
		);
		return proc.stdout as ReadableStream<Uint8Array>;
	}

	async exec(stack: string, service: string, command: string) {
		return this.run(
			`docker exec $(docker ps -q -f "name=${this.svc(stack, service)}" | head -1) ${command}`,
		);
	}

	async syncSecrets(stack: string, name: string, data: Record<string, string>) {
		// In Swarm, inject secrets as env vars on each service in the stack
		const envArgs = Object.entries(data)
			.map(([k, v]) => `--env-add ${k}=${v}`)
			.join(" ");
		const { ok } = await this.run(`docker service update ${envArgs} ${this.svc(stack, name)}`);
		return ok;
	}

	async deleteSecretKey(stack: string, name: string, key: string) {
		const { ok } = await this.run(`docker service update --env-rm ${key} ${this.svc(stack, name)}`);
		return ok;
	}

	async applyManifest(stack: string, manifest: string) {
		return this.run(
			`cat <<'COMPOSE_EOF' | docker stack deploy -c - ${stack}\n${manifest}\nCOMPOSE_EOF`,
		);
	}

	async deleteResource(stack: string, resource: string, name: string) {
		if (resource === "stack") {
			const { ok } = await this.run(`docker stack rm ${name}`);
			return ok;
		}
		const { ok } = await this.run(`docker service rm ${this.svc(stack, name)} 2>/dev/null || true`);
		return ok;
	}

	async runJob(stack: string, name: string, image: string, envFrom?: string) {
		const svcName = `${stack}_${name}`;
		// Cleanup previous run
		await this.run(`docker service rm ${svcName} 2>/dev/null || true`);

		const envArgs = envFrom
			? `--env-file <(docker service inspect --format '{{range .Spec.TaskTemplate.ContainerSpec.Env}}{{println .}}{{end}}' ${this.svc(stack, envFrom)} 2>/dev/null)`
			: "";

		const { ok } = await this.run(
			`docker service create --name ${svcName} --restart-condition=none --detach=false ${envArgs} ${image}`,
		);

		// Cleanup after completion
		await this.run(`docker service rm ${svcName} 2>/dev/null || true`);
		return ok;
	}
}
