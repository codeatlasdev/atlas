import type { RuntimeService } from "@atlas/api/types";
import { ssh } from "@atlas/ssh";

const VMM_SOCK = "/var/run/atlas-vmm.sock";

export class FirecrackerRuntime implements RuntimeService {
	readonly type = "firecracker" as const;

	constructor(private host: string) {}

	private async run(command: string) {
		return ssh(this.host, command);
	}

	/** Call atlas-vmm daemon via Unix socket */
	private async vmm(method: string, path: string, body?: unknown) {
		const bodyFlag = body ? `-d '${JSON.stringify(body)}'` : "";
		return this.run(
			`curl -sf --unix-socket ${VMM_SOCK} -X ${method} -H 'Content-Type: application/json' ${bodyFlag} http://localhost${path}`,
		);
	}

	async deploy(stack: string, service: string, image: string) {
		// 1. Pull image and extract filesystem into rootfs
		const { ok: buildOk } = await this.vmm("POST", "/rootfs/build", { stack, service, image });
		if (!buildOk) return false;

		// 2. Stop existing VM for this service (rolling update)
		await this.vmm("DELETE", `/vms/${stack}/${service}`);

		// 3. Start new VM with the built rootfs
		const { ok } = await this.vmm("POST", "/vms", {
			stack,
			service,
			rootfs: `/opt/atlas/firecracker/rootfs/${stack}-${service}.ext4`,
		});
		return ok;
	}

	async rolloutStatus(stack: string, service: string, timeoutSec = 120) {
		const deadline = Date.now() + timeoutSec * 1000;
		while (Date.now() < deadline) {
			const { ok, stdout } = await this.vmm("GET", `/vms/${stack}/${service}`);
			if (ok) {
				try {
					const vm = JSON.parse(stdout);
					if (vm.status === "running" && vm.healthy) return true;
					if (vm.status === "failed") return false;
				} catch {}
			}
			await new Promise((r) => setTimeout(r, 2000));
		}
		return false;
	}

	async scale(stack: string, service: string, replicas: number) {
		const { ok } = await this.vmm("POST", `/vms/${stack}/${service}/scale`, { replicas });
		return ok;
	}

	async getPods(stack: string) {
		const { stdout } = await this.vmm("GET", `/vms?stack=${stack}`);
		return stdout;
	}

	async streamLogs(
		stack: string,
		service: string,
		opts: { tail?: number; follow?: boolean } = {},
	): Promise<ReadableStream<Uint8Array>> {
		const tail = opts.tail ?? 100;
		const followFlag = opts.follow ? "&follow=true" : "";
		const cmd = `curl -sf --unix-socket ${VMM_SOCK} 'http://localhost/vms/${stack}/${service}/logs?tail=${tail}${followFlag}'`;

		const proc = Bun.spawn(
			["ssh", "-o", "StrictHostKeyChecking=accept-new", "-o", "ConnectTimeout=10", this.host, cmd],
			{ stdout: "pipe", stderr: "pipe" },
		);
		return proc.stdout as ReadableStream<Uint8Array>;
	}

	async exec(stack: string, service: string, command: string) {
		return this.run(
			`curl -sf --unix-socket ${VMM_SOCK} -X POST -H 'Content-Type: application/json' -d '${JSON.stringify({ command })}' 'http://localhost/vms/${stack}/${service}/exec'`,
		);
	}

	async syncSecrets(stack: string, name: string, data: Record<string, string>) {
		const { ok } = await this.vmm("POST", `/vms/${stack}/${name}/env`, data);
		return ok;
	}

	async deleteSecretKey(stack: string, name: string, key: string) {
		const { ok } = await this.vmm("DELETE", `/vms/${stack}/${name}/env/${key}`);
		return ok;
	}

	async applyManifest(stack: string, manifest: string) {
		return this.run(
			`curl -sf --unix-socket ${VMM_SOCK} -X POST -H 'Content-Type: application/json' -d '${JSON.stringify({ manifest })}' 'http://localhost/vms/${stack}/apply'`,
		);
	}

	async deleteResource(stack: string, resource: string, name: string) {
		if (resource === "stack") {
			const { ok } = await this.vmm("DELETE", `/vms/${stack}`);
			return ok;
		}
		const { ok } = await this.vmm("DELETE", `/vms/${stack}/${name}`);
		return ok;
	}

	async runJob(stack: string, name: string, image: string, envFrom?: string) {
		const { ok } = await this.vmm("POST", "/jobs", { stack, name, image, envFrom });
		return ok;
	}
}
