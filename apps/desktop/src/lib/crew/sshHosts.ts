/**
 * The servers of the user's `~/.ssh/config`, as the `ssh_hosts_list` command returns them (`ssh -G` already applied:
 * `Host *` defaults, `Match` blocks, `Include`). No Svelte/Tauri imports: stays bun-testable in isolation.
 */

/** Rust `atlas_core::ssh_config::SshHost`, field for field (its fixture is `__fixtures__/ssh-hosts.json`). */
export type SshHost = {
	alias: string;
	user: string;
	hostname: string;
	port: number;
	/** The first key file `ssh` would offer that exists on this Mac, as printed (`~/.ssh/id_work`). */
	identity_file: string | null;
	/** `user@alias`: what the SSH address field takes; ssh keeps applying the alias's own key, port and jump host. */
	address: string;
};

/** The picker row: the name, then where it goes (`prod · deploy@203.0.113.7:2222`). Typing filters on all of it. */
export function hostLabel(h: SshHost): string {
	return `${h.alias} · ${h.user}@${h.hostname}${h.port === 22 ? "" : `:${h.port}`}`;
}

/** The host the address and port fields currently spell, or `undefined` once the user edits either away from it. */
export function pickedHost(hosts: SshHost[], address: string, port: number): SshHost | undefined {
	return hosts.find((h) => h.address === address.trim() && h.port === port);
}
