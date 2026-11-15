/// The card a crewmate raises when a task is `waiting_action`: connect GitHub, give a secret
/// or a value, pick an option, sign in to an engine, approve an automation it proposes, or decide on work Atlas's guards refused.
/// Every kind is a title, an optional reason, its own body and a Submit / Cancel row; the
/// answers go out as `crewmate_action_answer`, a cancel as `crewmate_cancel`.
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/require_mux.dart';
import 'package:atlas_mobile/core/auth/device_auth.dart';
import 'package:atlas_mobile/core/protocol/crew_models.dart';
import 'package:atlas_mobile/core/protocol/crew_ops_models.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/attention_card.dart';
import 'package:atlas_mobile/features/crew/chat/attachments.dart';
import 'package:atlas_mobile/features/crew/chat/task_images.dart';
import 'package:atlas_mobile/features/crew/crew_provider.dart';
import 'package:atlas_mobile/features/crew/ops/engine_signin.dart';
import 'package:atlas_mobile/features/crew/widgets/github_device_login.dart';
import 'package:atlas_mobile/features/crew/widgets/pill_buttons.dart';

class TaskActionCard extends ConsumerStatefulWidget {
  const TaskActionCard({
    super.key,
    required this.task,
    required this.device,
    required this.crewmateId,
  });

  final CrewmateTask task;
  final MacDevice device;
  final String crewmateId;

  @override
  ConsumerState<TaskActionCard> createState() => _TaskActionCardState();
}

class _TaskActionCardState extends ConsumerState<TaskActionCard> {
  final _controller = TextEditingController();
  bool _submitting = false;
  late final _login = GithubDeviceLogin(onChanged: () => setState(() {}));

  /// A marked screenshot the owner added to a retryable card (refused / prove_ui), sent with `retry`.
  final _images = <PendingImage>[];
  String? _error;

  @override
  void dispose() {
    _controller.dispose();
    _login.dispose();
    super.dispose();
  }

  Future<void> _submit(String value) async {
    if (value.isEmpty || _submitting) return;
    final mux = liveMux(ref, widget.device);
    if (mux == null) return;
    setState(() {
      _submitting = true;
      _error = null;
    });
    try {
      // A secret is typed on the phone and stored on the crew server for good: prove it is the owner first,
      // and send nothing when they do not (or the device cannot ask).
      final action = widget.task.action;
      if (action?.kind == 'provide_secret' && !await _confirmSecret(action!)) {
        throw 'Face ID is needed to send a secret';
      }
      // Images only ride on a retry (hostd refuses them on an override); big ones go up in pieces first.
      var n = 0;
      final stamp = DateTime.now().microsecondsSinceEpoch;
      final attachments = value == 'retry' && _images.isNotEmpty
          ? await toSent(
              List.of(_images),
              mux.call,
              () => '$stamp-${n++}-up', // one upload id per image
              crewmateId: widget.crewmateId,
            )
          : const <Map<String, dynamic>>[];
      final resp = await mux.call('crewmate_action_answer', {
        'id': widget.crewmateId,
        'task': widget.task.id,
        'value': value,
        if (attachments.isNotEmpty) 'attachments': attachments,
      });
      if (resp['ok'] != true) {
        throw StateError(resp['error'] as String? ?? 'the server refused');
      }
      _controller.clear();
      _images.clear();
    } on Object catch (e) {
      // The card stays: what the server said, not a silent nothing.
      if (mounted) setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _submitting = false);
    }
  }

  /// Face ID (or the device passcode) before a secret is sent. A device that cannot ask says why, and
  /// nothing is sent.
  Future<bool> _confirmSecret(TaskAction action) async {
    try {
      return await ref.read(deviceAuthProvider)(
        'Send ${action.name ?? 'this secret'} to your crew server',
      );
    } on Object catch (e) {
      throw 'Face ID is unavailable ($e): the secret was not sent';
    }
  }

  Future<void> _addImages() async {
    final source = await showModalBottomSheet<bool>(
      context: context,
      backgroundColor: AtlasColors.of(context).overlay,
      builder: (ctx) => SafeArea(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            ListTile(
              leading: const Icon(Icons.photo_camera_outlined),
              title: const Text('Take photo'),
              onTap: () => Navigator.of(ctx).pop(true),
            ),
            ListTile(
              leading: const Icon(Icons.photo_library_outlined),
              title: const Text('Choose from library'),
              onTap: () => Navigator.of(ctx).pop(false),
            ),
          ],
        ),
      ),
    );
    if (source == null || !mounted) return;
    final picker = ref.read(imagePickerProvider);
    final List<PickedImage> picked;
    try {
      picked = source ? await picker.camera() : await picker.gallery();
    } on Object catch (e) {
      if (mounted) setState(() => _error = 'Could not open the picker: $e');
      return;
    }
    if (!mounted) return;
    final r = addImages(_images, picked);
    setState(() {
      _images
        ..clear()
        ..addAll(r.images);
      _error = r.errors.isEmpty ? null : r.errors.join('\n');
    });
  }

  void _submitText() {
    final v = _controller.text.trim();
    if (v.isNotEmpty) _submit(v);
  }

  Future<void> _cancel() async {
    final mux = liveMux(ref, widget.device);
    if (mux == null) return;
    await mux.call('crewmate_cancel', {
      'id': widget.crewmateId,
      'task': widget.task.id,
    });
    if (mounted) {
      ref.invalidate(crewmateTasksProvider((widget.device, widget.crewmateId)));
    }
  }

  Future<void> _startGithubLogin() async {
    final mux = liveMux(ref, widget.device);
    if (mux == null) return;
    await _login.start(mux, _pollGithubLogin);
  }

  Future<void> _pollGithubLogin() async {
    final mux = liveMux(ref, widget.device);
    if (mux == null) return;
    final (result, _) = await _login.pollOnce(mux);
    // Wake the runner once the user has authorized.
    if (result == GithubPoll.done && mounted) {
      await mux.call('crew_github_connect_resume', {});
    }
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final action = widget.task.action;
    if (action == null) return const SizedBox.shrink();

    final busy = _submitting;
    final cardContent = switch (action.kind) {
      'connect_github' => _connectGithub(c, action, busy),
      'provide_secret' => _textCard(
        c,
        action,
        busy,
        title: action.label ?? 'Secret required',
        reason: true,
        hint: 'Enter value',
        obscure: true,
      ),
      'input' => _textCard(
        c,
        action,
        busy,
        title: action.label ?? 'Input required',
        reason: true,
        hint: action.placeholder ?? '',
        submitOnEnter: true,
      ),
      'choose' => _choose(c, action, busy),
      'sign_in_engine' => _signInEngine(c, action, busy),
      'add_trigger' => _addTrigger(c, action, busy),
      'refused' => _retryable(
        c,
        busy,
        title: 'This work was refused',
        reasons: action.reasons,
      ),
      'prove_ui' => _retryable(
        c,
        busy,
        title: 'Could not prove this UI work',
        reasons: [if (action.reason != null) action.reason!],
      ),
      _ => _textCard(c, action, busy, title: action.label ?? 'Response needed'),
    };

    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          const SizedBox(width: 36), // avatar placeholder alignment
          Flexible(child: AttentionCard(bubble: true, child: cardContent)),
        ],
      ),
    );
  }

  // ── shared pieces ──────────────────────────────────────────────────────────

  Widget _title(AtlasColors c, String text) => Text(
    text,
    style: TextStyle(fontSize: 13, fontWeight: FontWeight.w600, color: c.text),
  );

  /// The agent's reason under a title, when it gave one.
  List<Widget> _reason(AtlasColors c, TaskAction action) => [
    if (action.reason != null) ...[
      const SizedBox(height: 4),
      Text(action.reason!, style: TextStyle(fontSize: 12, color: c.muted)),
    ],
  ];

  /// Primary + Cancel, the row every kind but `choose` ends with.
  Widget _actions(
    String primary,
    VoidCallback? onPrimary, {
    String cancel = 'Cancel',
    VoidCallback? onCancel,
  }) => Row(
    children: [
      WarnPillButton(label: primary, onPressed: onPrimary),
      const SizedBox(width: 8),
      OutlinePillButton(
        label: cancel,
        fontSize: 13,
        onPressed: _submitting ? null : (onCancel ?? _cancel),
      ),
    ],
  );

  Widget _field(
    AtlasColors c, {
    String? hint,
    bool obscure = false,
    ValueChanged<String>? onSubmitted,
  }) => TextField(
    controller: _controller,
    obscureText: obscure,
    style: TextStyle(fontSize: 14, color: c.text),
    decoration: InputDecoration(
      hintText: hint,
      hintStyle: TextStyle(color: c.faint, fontSize: 14),
      contentPadding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
      border: OutlineInputBorder(
        borderRadius: BorderRadius.circular(8),
        borderSide: BorderSide(color: c.borderSubtle),
      ),
      enabledBorder: OutlineInputBorder(
        borderRadius: BorderRadius.circular(8),
        borderSide: BorderSide(color: c.borderSubtle),
      ),
    ),
    onSubmitted: onSubmitted,
  );

  // ── kinds ──────────────────────────────────────────────────────────────────

  /// A title, a text field and Submit / Cancel: `provide_secret`, `input` and the fallback.
  Widget _textCard(
    AtlasColors c,
    TaskAction action,
    bool busy, {
    required String title,
    bool reason = false,
    String? hint,
    bool obscure = false,
    bool submitOnEnter = false,
  }) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      _title(c, title),
      if (reason) ..._reason(c, action),
      const SizedBox(height: 8),
      _field(
        c,
        hint: hint,
        obscure: obscure,
        onSubmitted: submitOnEnter
            ? (v) {
                if (v.trim().isNotEmpty) _submit(v.trim());
              }
            : null,
      ),
      if (_error != null) ...[
        const SizedBox(height: 6),
        Text(_error!, style: TextStyle(fontSize: 12, color: c.error)),
      ],
      const SizedBox(height: 8),
      _actions('Submit', busy ? null : _submitText),
    ],
  );

  Widget _connectGithub(AtlasColors c, TaskAction action, bool busy) {
    final flow = _login.flow;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _title(c, 'Connect GitHub to continue'),
        ..._reason(c, action),
        const SizedBox(height: 10),
        if (flow != null) ...[
          Text(
            'Enter this code at ${flow.verificationUri}',
            style: TextStyle(fontSize: 12, color: c.muted),
          ),
          const SizedBox(height: 6),
          Row(
            children: [
              Text(
                flow.userCode,
                style: TextStyle(
                  fontSize: 20,
                  fontWeight: FontWeight.w700,
                  letterSpacing: 2,
                  fontFamily: monoFamily,
                  fontFamilyFallback: monoFallback,
                  color: c.text,
                ),
              ),
              const SizedBox(width: 8),
              OutlinedButton(
                onPressed: () {
                  // clipboard
                },
                style: OutlinedButton.styleFrom(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 10,
                    vertical: 4,
                  ),
                  minimumSize: Size.zero,
                  tapTargetSize: MaterialTapTargetSize.padded,
                  shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(8),
                  ),
                ),
                child: const Text('Copy', style: TextStyle(fontSize: 12)),
              ),
            ],
          ),
          if (_login.polling) ...[
            const SizedBox(height: 6),
            Text(
              'Waiting for authorization…',
              style: TextStyle(fontSize: 12, color: c.muted),
            ),
          ],
          const SizedBox(height: 8),
          OutlinePillButton(
            label: 'Cancel',
            fontSize: 13,
            hPad: 12,
            vPad: 4,
            onPressed: _login.cancel,
          ),
        ] else
          _actions(
            'Connect',
            busy ? null : _startGithubLogin,
            cancel: 'Cancel task',
          ),
      ],
    );
  }

  Widget _choose(AtlasColors c, TaskAction action, bool busy) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      _title(c, action.question ?? 'Choose an option'),
      const SizedBox(height: 8),
      for (int i = 0; i < action.options.length; i++) ...[
        OutlinedButton(
          onPressed: busy ? null : () => _submit(action.options[i]),
          style: OutlinedButton.styleFrom(
            foregroundColor: c.text,
            side: BorderSide(color: c.borderSubtle),
            padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 8),
            minimumSize: const Size(double.infinity, 48),
            tapTargetSize: MaterialTapTargetSize.padded,
            shape: RoundedRectangleBorder(
              borderRadius: BorderRadius.circular(10),
            ),
            alignment: Alignment.centerLeft,
          ),
          child: Row(
            children: [
              Container(
                width: 22,
                height: 22,
                alignment: Alignment.center,
                decoration: BoxDecoration(
                  color: c.ground,
                  borderRadius: BorderRadius.circular(4),
                  border: Border.all(color: c.borderSubtle),
                ),
                child: Text(
                  String.fromCharCode(65 + i),
                  style: TextStyle(
                    fontSize: 11,
                    fontWeight: FontWeight.w600,
                    color: c.muted,
                  ),
                ),
              ),
              const SizedBox(width: 10),
              Flexible(
                child: Text(
                  action.options[i],
                  style: TextStyle(fontSize: 14, color: c.text),
                ),
              ),
            ],
          ),
        ),
        if (i < action.options.length - 1) const SizedBox(height: 4),
      ],
      const SizedBox(height: 8),
      OutlinePillButton(
        label: 'Cancel task',
        fontSize: 13,
        onPressed: busy ? null : _cancel,
      ),
    ],
  );

  Widget _signInEngine(AtlasColors c, TaskAction action, bool busy) {
    final engine = action.engine ?? 'engine';
    // Only the engines hostd can sign in get the button; any other name keeps the Settings text.
    final canSignInHere = engineLabels.containsKey(engine);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _title(c, 'Sign in to ${engineLabels[engine] ?? engine}'),
        const SizedBox(height: 4),
        Text(
          canSignInHere
              ? 'Tap Sign in here to open its sign-in on the crew server, finish it, then tap Done.'
              : 'Sign in to $engine on the crew server, then tap Done.',
          style: TextStyle(fontSize: 12, color: c.muted),
        ),
        const SizedBox(height: 10),
        if (canSignInHere) ...[
          WarnPillButton(
            key: const Key('sign-in-here'),
            label: 'Sign in here',
            onPressed: busy
                ? null
                : () => openEngineSignin(context, ref, widget.device, engine),
          ),
          const SizedBox(height: 8),
        ],
        _actions('Done', busy ? null : () => _submit('done')),
      ],
    );
  }

  /// A card that can be retried: Atlas's guards refused the work, or a UI task finished without two captures.
  /// The captures the agent took (tap to enlarge), the reasons, a marked screenshot the owner may add, then
  /// Retry (the task runs again with the reasons in its prompt and the screenshot staged for the agent) or
  /// Override (accept the work as it is; images never go with it).
  Widget _retryable(
    AtlasColors c,
    bool busy, {
    required String title,
    required List<String> reasons,
  }) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      _title(c, title),
      TaskImageRow(
        images: widget.task.agentImages,
        taskId: widget.task.id,
        device: widget.device,
        crewmateId: widget.crewmateId,
        alignEnd: false,
      ),
      const SizedBox(height: 6),
      for (final reason in reasons)
        Padding(
          padding: const EdgeInsets.only(bottom: 2),
          child: Text(
            '• $reason',
            style: TextStyle(fontSize: 12, color: c.muted),
          ),
        ),
      const SizedBox(height: 8),
      if (_images.isNotEmpty)
        Wrap(
          spacing: 6,
          runSpacing: 4,
          children: [
            for (var i = 0; i < _images.length; i++)
              InputChip(
                label: Text(
                  _images[i].name,
                  style: const TextStyle(fontSize: 12),
                ),
                onDeleted: busy
                    ? null
                    : () => setState(() => _images.removeAt(i)),
                deleteButtonTooltipMessage: 'Remove ${_images[i].name}',
              ),
          ],
        ),
      OutlinePillButton(
        label: 'Add a screenshot',
        fontSize: 13,
        onPressed: busy ? null : _addImages,
      ),
      if (_error != null) ...[
        const SizedBox(height: 6),
        Text(_error!, style: TextStyle(fontSize: 12, color: c.error)),
      ],
      const SizedBox(height: 8),
      Row(
        children: [
          WarnPillButton(
            label: 'Retry',
            onPressed: busy ? null : () => _submit('retry'),
          ),
          const SizedBox(width: 8),
          OutlinePillButton(
            label: 'Override',
            fontSize: 13,
            onPressed: busy ? null : () => _submit('override'),
          ),
        ],
      ),
    ],
  );

  /// The crewmate proposes an automation; the user adds it or skips.
  Widget _addTrigger(AtlasColors c, TaskAction action, bool busy) {
    final triggerKind = action.triggerKind;
    final triggerPrompt = action.triggerPrompt;
    final resolved = action.reason;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Icon(Icons.schedule_outlined, size: 14, color: c.warning),
            const SizedBox(width: 6),
            Flexible(
              child: Text(
                '${action.name ?? widget.crewmateId} wants to add an automation',
                style: TextStyle(
                  fontSize: 13,
                  fontWeight: FontWeight.w600,
                  color: c.warning,
                ),
                overflow: TextOverflow.ellipsis,
              ),
            ),
          ],
        ),
        const SizedBox(height: 8),
        if (triggerKind != null) ...[
          Text(
            _describeTriggerKind(triggerKind),
            style: TextStyle(fontSize: 13, color: c.text),
          ),
          const SizedBox(height: 2),
        ],
        if (triggerPrompt != null && triggerPrompt.isNotEmpty) ...[
          Text(
            triggerPrompt,
            style: TextStyle(fontSize: 12, color: c.muted),
            maxLines: 2,
            overflow: TextOverflow.ellipsis,
          ),
          const SizedBox(height: 2),
        ],
        if (resolved != null) ...[
          Text(resolved, style: TextStyle(fontSize: 12, color: c.muted)),
          const SizedBox(height: 6),
        ] else
          const SizedBox(height: 6),
        Row(
          children: [
            WarnPillButton(
              label: 'Add',
              onPressed: busy ? null : () => _submit('add'),
            ),
            const SizedBox(width: 8),
            OutlinePillButton(
              label: 'Not now',
              fontSize: 13,
              onPressed: busy ? null : () => _submit('skip'),
            ),
          ],
        ),
      ],
    );
  }
}

String _describeTriggerKind(TriggerKind kind) => switch (kind) {
  ScheduleTrigger(:final cron) => 'Schedule: $cron',
  IntervalTrigger(:final everySecs) => 'Every ${_formatInterval(everySecs)}',
  WebhookTrigger() => 'Webhook',
  GithubTrigger(:final repo, :final events) =>
    'GitHub: $repo (${events.join(', ')})',
  FileTrigger(:final path, :final glob) =>
    'File: $path${glob != null ? '/$glob' : ''}',
  CrewmateTrigger(:final fromCrewmate, :final on) =>
    'After $fromCrewmate ($on)',
  ServiceTrigger(:final service, :final on) => 'Service $service ($on)',
};

String _formatInterval(int secs) {
  if (secs < 60) return '${secs}s';
  if (secs < 3600) return '${secs ~/ 60}m';
  return '${secs ~/ 3600}h';
}
