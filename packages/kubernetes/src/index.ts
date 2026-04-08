import { ssh } from "@atlas/ssh"

export class KubernetesService {
	constructor(
		private host: string,
		private kubeconfig: string = "/etc/rancher/k3s/k3s.yaml",
	) {}

	private async run(command: string) {
		return ssh(this.host, `export KUBECONFIG=${this.kubeconfig}\n${command}`)
	}

	async kubectl(args: string) {
		return this.run(`kubectl ${args}`)
	}

	async setImage(namespace: string, deployment: string, container: string, image: string) {
		const { ok } = await this.kubectl(`-n ${namespace} set image deployment/${deployment} ${container}=${image}`)
		return ok
	}

	async applyStdin(namespace: string, yaml: string) {
		return this.run(`cat <<'YAML_EOF' | kubectl -n ${namespace} apply -f -\n${yaml}\nYAML_EOF`)
	}

	async deleteResource(namespace: string, resource: string, name: string) {
		const { ok } = await this.kubectl(`-n ${namespace} delete ${resource} ${name} --ignore-not-found`)
		return ok
	}

	async rolloutStatus(namespace: string, deployment: string) {
		const { ok } = await this.kubectl(`-n ${namespace} rollout status deployment/${deployment} --timeout=120s`)
		return ok
	}

	async getPods(namespace: string) {
		const { stdout } = await this.kubectl(`-n ${namespace} get pods -o wide`)
		return stdout
	}

	async syncSecret(namespace: string, name: string, data: Record<string, string>) {
		const patches = Object.entries(data)
			.map(([k, v]) => `"${k}":"${Buffer.from(v).toString("base64")}"`)
			.join(",")
		const { ok: patchOk } = await this.run(
			`kubectl -n ${namespace} patch secret ${name} -p '{"data":{${patches}}}' 2>/dev/null`,
		)
		if (patchOk) return true

		const literals = Object.entries(data)
			.map(([k, v]) => `--from-literal=${k}=${v}`)
			.join(" ")
		const { ok } = await this.run(`kubectl -n ${namespace} create secret generic ${name} ${literals}`)
		return ok
	}

	async deleteSecretKey(namespace: string, name: string, key: string) {
		const { ok } = await this.run(
			`kubectl -n ${namespace} patch secret ${name} --type=json -p '[{"op":"remove","path":"/data/${key}"}]'`,
		)
		return ok
	}

	async streamLogs(
		namespace: string,
		deployment: string,
		opts: { tail?: number; follow?: boolean } = {},
	): Promise<ReadableStream<Uint8Array>> {
		const tail = opts.tail ?? 100
		const followFlag = opts.follow ? "-f" : ""
		const cmd = `export KUBECONFIG=${this.kubeconfig}; kubectl -n ${namespace} logs deployment/${deployment} --tail=${tail} ${followFlag} --all-containers 2>&1`

		const proc = Bun.spawn(
			["ssh", "-o", "StrictHostKeyChecking=accept-new", "-o", "ConnectTimeout=10", this.host, cmd],
			{ stdout: "pipe", stderr: "pipe" },
		)

		return proc.stdout as ReadableStream<Uint8Array>
	}
}
