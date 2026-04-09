import { ssh } from "@atlas/ssh";
import type { RuntimeService } from "./index";

const KUBECONFIG = "/etc/rancher/k3s/k3s.yaml";

export class K3sRuntime implements RuntimeService {
	readonly type = "k3s" as const;

	constructor(private host: string) {}

	private async run(command: string) {
		return ssh(this.host, `export KUBECONFIG=${KUBECONFIG}\n${command}`);
	}

	private async kubectl(args: string) {
		return this.run(`kubectl ${args}`);
	}

	async deploy(stack: string, service: string, image: string) {
		const { ok } = await this.kubectl(
			`-n ${stack} set image deployment/${service} ${service}=${image}`,
		);
		return ok;
	}

	async rolloutStatus(stack: string, service: string, timeoutSec = 120) {
		const { ok } = await this.kubectl(
			`-n ${stack} rollout status deployment/${service} --timeout=${timeoutSec}s`,
		);
		return ok;
	}

	async scale(stack: string, service: string, replicas: number) {
		const { ok } = await this.kubectl(
			`-n ${stack} scale deployment/${service} --replicas=${replicas}`,
		);
		return ok;
	}

	async getPods(stack: string) {
		const { stdout } = await this.kubectl(`-n ${stack} get pods -o wide`);
		return stdout;
	}

	async streamLogs(
		stack: string,
		service: string,
		opts: { tail?: number; follow?: boolean } = {},
	): Promise<ReadableStream<Uint8Array>> {
		const tail = opts.tail ?? 100;
		const followFlag = opts.follow ? "-f" : "";
		const cmd = `export KUBECONFIG=${KUBECONFIG}; kubectl -n ${stack} logs deployment/${service} --tail=${tail} ${followFlag} --all-containers 2>&1`;

		const proc = Bun.spawn(
			["ssh", "-o", "StrictHostKeyChecking=accept-new", "-o", "ConnectTimeout=10", this.host, cmd],
			{ stdout: "pipe", stderr: "pipe" },
		);
		return proc.stdout as ReadableStream<Uint8Array>;
	}

	async exec(stack: string, service: string, command: string) {
		return this.kubectl(`-n ${stack} exec deployment/${service} -- ${command}`);
	}

	async syncSecrets(stack: string, name: string, data: Record<string, string>) {
		const patches = Object.entries(data)
			.map(([k, v]) => `"${k}":"${Buffer.from(v).toString("base64")}"`)
			.join(",");
		const { ok: patchOk } = await this.run(
			`kubectl -n ${stack} patch secret ${name} -p '{"data":{${patches}}}' 2>/dev/null`,
		);
		if (patchOk) return true;

		const literals = Object.entries(data)
			.map(([k, v]) => `--from-literal=${k}=${v}`)
			.join(" ");
		const { ok } = await this.run(`kubectl -n ${stack} create secret generic ${name} ${literals}`);
		return ok;
	}

	async deleteSecretKey(stack: string, name: string, key: string) {
		const { ok } = await this.run(
			`kubectl -n ${stack} patch secret ${name} --type=json -p '[{"op":"remove","path":"/data/${key}"}]'`,
		);
		return ok;
	}

	async applyManifest(stack: string, manifest: string) {
		return this.run(`cat <<'YAML_EOF' | kubectl -n ${stack} apply -f -\n${manifest}\nYAML_EOF`);
	}

	async deleteResource(stack: string, resource: string, name: string) {
		const { ok } = await this.kubectl(`-n ${stack} delete ${resource} ${name} --ignore-not-found`);
		return ok;
	}

	async runJob(stack: string, name: string, image: string, envFrom?: string) {
		await this.deleteResource(stack, "job", name);
		const envSpec = envFrom
			? `\n          envFrom:\n            - secretRef:\n                name: ${envFrom}`
			: "";
		const yaml = `apiVersion: batch/v1
kind: Job
metadata:
  name: ${name}
spec:
  backoffLimit: 3
  ttlSecondsAfterFinished: 300
  template:
    spec:
      restartPolicy: OnFailure
      imagePullSecrets:
        - name: ghcr-auth
      containers:
        - name: ${name}
          image: ${image}${envSpec}`;

		const { ok } = await this.applyManifest(stack, yaml);
		return ok;
	}
}
