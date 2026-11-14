/**
 * Owns: the runes state of one crewmate chat: composer text and send, inline approvals, folded/dismissed replies,
 * the action-card inputs, scroll-follow / jump-to-present and the Board "focus this task" flash.
 * Components read the fields; nothing here touches the DOM except the thread element it is handed.
 */
import { toast } from "@codeatlasdev/ui";
import { errText } from "#lib/api.ts";
import { hostCall } from "#lib/host.ts";
import { crew } from "#lib/stores/crew.svelte.ts";
import { expandKey, withKey } from "./taskView.ts";

/** Host args for a call on the selected crew server ("" is the local hostd). */
function hostArgs(): { host?: string } {
	const hostKey = crew.selected?.hostKey ?? "";
	return hostKey ? { host: hostKey } : {};
}

export class ChatState {
	// composer
	prompt = $state("");
	sending = $state(false);
	composerFocused = $state(false);

	// approvals answered in this view (req ids); reset when the selected crewmate changes
	approvedReqs = $state<Set<string>>(new Set());

	// folded / full / dismissed flags of replies, see expandKey
	expandedResults = $state<Set<string>>(new Set());

	// scroll
	threadEl = $state<HTMLElement | null>(null);
	atBottom = $state(true);
	jumpVisible = $state(false);
	/** Task id flashing after a Board card opened it. */
	flashTask = $state<string | null>(null);

	// action cards: taskId -> value (secrets are never logged)
	actionInputs = $state<Record<string, string>>({});
	actionSecrets = $state<Record<string, string>>({});
	actionSubmitting = $state<Set<string>>(new Set());

	// ─── composer ──────────────────────────────────────────────────────────

	async send(): Promise<void> {
		if (!this.prompt.trim() || this.sending) return;
		this.sending = true;
		try {
			await crew.sendTask(this.prompt.trim());
			this.prompt = "";
		} catch (e) {
			toast.error(errText(e));
		} finally {
			this.sending = false;
		}
	}

	/** Put `text` in the composer and send it (ask option, typed answer, retry). A failure leaves it in the box. */
	sendText(text: string): Promise<void> {
		this.prompt = text;
		return this.send();
	}

	// ─── approvals ─────────────────────────────────────────────────────────

	resetApprovals(): void {
		this.approvedReqs = new Set();
	}

	async approval(paneKey: string, req: string, allow: boolean): Promise<void> {
		await crew
			.approve(paneKey, req, allow)
			.then(() => {
				this.approvedReqs = withKey(this.approvedReqs, req, true);
			})
			.catch((e) => toast.error(errText(e)));
	}

	// ─── replies: fold, full text, dismissed ask ────────────────────────────

	setExpanded(key: string, on: boolean): void {
		this.expandedResults = withKey(this.expandedResults, key, on);
	}

	isDismissed(taskId: string): boolean {
		return this.expandedResults.has(expandKey.dismissed(taskId));
	}

	// ─── action cards ──────────────────────────────────────────────────────

	async answerAction(taskId: string, crewmateId: string, value: string): Promise<void> {
		this.actionSubmitting = withKey(this.actionSubmitting, taskId, true);
		try {
			await hostCall("crewmate_action_answer", { ...hostArgs(), id: crewmateId, task: taskId, value });
			const i = { ...this.actionInputs };
			delete i[taskId];
			this.actionInputs = i;
			const s = { ...this.actionSecrets };
			delete s[taskId];
			this.actionSecrets = s;
		} finally {
			this.actionSubmitting = withKey(this.actionSubmitting, taskId, false);
		}
	}

	async cancelAction(taskId: string, crewmateId: string): Promise<void> {
		await hostCall("crewmate_cancel", { ...hostArgs(), id: crewmateId, task: taskId });
	}

	// ─── scroll ────────────────────────────────────────────────────────────

	onThreadScroll(): void {
		if (!this.threadEl) return;
		const { scrollTop, scrollHeight, clientHeight } = this.threadEl;
		this.atBottom = scrollHeight - scrollTop - clientHeight < 60;
		this.jumpVisible = !this.atBottom;
	}

	jumpToBottom(): void {
		this.threadEl?.scrollTo({ top: this.threadEl.scrollHeight, behavior: "smooth" });
	}

	/** Stick to the bottom when new tasks arrive, unless a Board card asked for one task (that wins). */
	followBottom(): void {
		if (crew.focusTask) return;
		if (this.atBottom && this.threadEl) {
			setTimeout(() => this.threadEl?.scrollTo({ top: this.threadEl.scrollHeight }), 0);
		}
	}

	/**
	 * Opened from a Board card: scroll to that task once it is rendered, and flash it so the eye finds it.
	 * `taskIds` are the ids in the thread, oldest first.
	 */
	focusRequestedTask(taskIds: string[]): void {
		const id = crew.focusTask;
		if (!id || !this.threadEl) return;
		if (!taskIds.includes(id)) {
			// Loaded and still not there (older than the chat's window): give up, or the chat would stop following new
			// messages, since the stick-to-bottom effect waits while a focus is pending.
			if (!crew.tasksLoading && taskIds.length > 0) crew.focusTask = null;
			return;
		}
		// Not "at the bottom" any more: clearing focusTask re-runs the stick-to-bottom effect, which put the
		// thread straight back at the end.
		this.atBottom = false;
		requestAnimationFrame(() => {
			this.threadEl?.querySelector(`[data-task-id="${CSS.escape(id)}"]`)?.scrollIntoView({ block: "center" });
			this.flashTask = id;
			crew.focusTask = null;
			setTimeout(() => this.flashTask === id && (this.flashTask = null), 1600);
		});
	}
}
