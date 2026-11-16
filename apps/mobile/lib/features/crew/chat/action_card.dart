/// The card a crewmate raises when a task is `waiting_action`: connect GitHub, give a secret
/// or a value, pick an option, sign in to an engine, approve an automation it proposes, or decide on work Atlas's guards refused.
/// Every kind is a title, an optional reason, its own body and a Submit / Cancel row; the
/// answers go out as `crewmate_action_answer`, a cancel as `crewmate_cancel`.
///
/// This file holds the state (what is typed, what is in flight, the last error) and the
/// calls; `action/` holds one body per kind.
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/auth/device_auth.dart';
import 'package:atlas_mobile/core/connection/require_mux.dart';
import 'package:atlas_mobile/core/protocol/crew_task.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/chat/action/action_parts.dart';
import 'package:atlas_mobile/features/crew/chat/action/choose_action.dart';
import 'package:atlas_mobile/features/crew/chat/action/engine_action.dart';
import 'package:atlas_mobile/features/crew/chat/action/github_action.dart';
import 'package:atlas_mobile/features/crew/chat/action/retry_action.dart';
import 'package:atlas_mobile/features/crew/chat/action/text_action.dart';
import 'package:atlas_mobile/features/crew/chat/action/trigger_action.dart';
import 'package:atlas_mobile/features/crew/chat/attachments.dart';
import 'package:atlas_mobile/features/crew/chat/crewmate_card_frame.dart';
import 'package:atlas_mobile/features/crew/chat/image_source.dart';
import 'package:atlas_mobile/features/crew/crew_provider.dart';
import 'package:atlas_mobile/features/crew/ops/engine_signin.dart';
import 'package:atlas_mobile/features/crew/widgets/github_device_login.dart';

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
    setState(() {
      _submitting = true;
      _error = null;
    });
    try {
      final mux = await requireMuxOf(ref, widget.device);
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
      if (mounted) setState(() => _error = errorText(e));
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
    final source = await askPhotoSource(
      context,
      libraryLabel: 'Choose from library',
    );
    if (source == null || !mounted) return;
    final List<PickedImage> picked;
    try {
      picked = await pickPhotos(ref, source);
    } on Object catch (e) {
      if (mounted) setState(() => _error = 'Could not open the picker: $e');
      return;
    }
    if (!mounted) return;
    setState(() => _error = takePicked(_images, picked));
  }

  void _submitText() {
    final v = _controller.text.trim();
    if (v.isNotEmpty) _submit(v);
  }

  Future<void> _cancel() async {
    setState(() => _error = null);
    try {
      final mux = await requireMuxOf(ref, widget.device);
      await mux.call('crewmate_cancel', {
        'id': widget.crewmateId,
        'task': widget.task.id,
      });
    } on DeviceUnavailable catch (e) {
      if (mounted) setState(() => _error = e.message);
      return;
    }
    if (mounted) {
      ref.invalidate(crewmateTasksProvider((widget.device, widget.crewmateId)));
    }
  }

  Future<void> _startGithubLogin() async {
    setState(() => _error = null);
    try {
      final mux = await requireMuxOf(ref, widget.device);
      await _login.start(mux, _pollGithubLogin);
    } on DeviceUnavailable catch (e) {
      if (mounted) setState(() => _error = e.message);
    }
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

  /// Primary + Cancel, the row every kind but `choose` ends with.
  Widget _buttons(
    String primary,
    VoidCallback? onPrimary, {
    String cancel = 'Cancel',
  }) => ActionButtons(
    primary: primary,
    onPrimary: onPrimary,
    cancel: cancel,
    onCancel: _submitting ? null : _cancel,
  );

  @override
  Widget build(BuildContext context) {
    final action = widget.task.action;
    if (action == null) return const SizedBox.shrink();

    final busy = _submitting;
    final cardContent = switch (action.kind) {
      'connect_github' => ConnectGithubAction(
        action: action,
        flow: _login.flow,
        polling: _login.polling,
        onCancelFlow: _login.cancel,
        buttons: _buttons(
          'Connect',
          busy ? null : _startGithubLogin,
          cancel: 'Cancel task',
        ),
      ),
      'provide_secret' => _text(
        action,
        busy,
        title: action.label ?? 'Secret required',
        reason: true,
        hint: 'Enter value',
        obscure: true,
      ),
      'input' => _text(
        action,
        busy,
        title: action.label ?? 'Input required',
        reason: true,
        hint: action.placeholder ?? '',
        submitOnEnter: true,
      ),
      'choose' => ChooseAction(
        action: action,
        busy: busy,
        onPick: _submit,
        onCancel: _cancel,
      ),
      'sign_in_engine' => SignInEngineAction(
        action: action,
        busy: busy,
        onSignIn: (engine) =>
            openEngineSignin(context, ref, widget.device, engine),
        buttons: _buttons('Done', busy ? null : () => _submit('done')),
      ),
      'add_trigger' => AddTriggerAction(
        action: action,
        crewmateId: widget.crewmateId,
        busy: busy,
        onAdd: () => _submit('add'),
        onSkip: () => _submit('skip'),
      ),
      'refused' => _retry(
        busy,
        title: 'This work was refused',
        reasons: action.reasons,
      ),
      'prove_ui' => _retry(
        busy,
        title: 'Could not prove this UI work',
        reasons: [if (action.reason != null) action.reason!],
      ),
      _ => _text(action, busy, title: action.label ?? 'Response needed'),
    };

    return CrewmateCardFrame(
      children: [
        cardContent,
        // What went wrong with the last tap (offline, refused): one line for every kind of card.
        if (_error != null) ...[
          const SizedBox(height: Space.sm),
          CardError(_error!),
        ],
      ],
    );
  }

  Widget _text(
    TaskAction action,
    bool busy, {
    required String title,
    bool reason = false,
    String? hint,
    bool obscure = false,
    bool submitOnEnter = false,
  }) => TextAction(
    title: title,
    reason: reason ? action : null,
    controller: _controller,
    hint: hint,
    obscure: obscure,
    onSubmitted: submitOnEnter
        ? (v) {
            if (v.trim().isNotEmpty) _submit(v.trim());
          }
        : null,
    buttons: _buttons('Submit', busy ? null : _submitText),
  );

  Widget _retry(
    bool busy, {
    required String title,
    required List<String> reasons,
  }) => RetryAction(
    title: title,
    reasons: reasons,
    task: widget.task,
    device: widget.device,
    crewmateId: widget.crewmateId,
    images: _images,
    busy: busy,
    onRemoveImage: (i) => setState(() => _images.removeAt(i)),
    onAddImage: _addImages,
    onRetry: () => _submit('retry'),
    onOverride: () => _submit('override'),
  );
}
