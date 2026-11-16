/// The card for a question the agent is blocked on (`ask_user`): the question, one button per option and,
/// when the agent allows it, a free-text field. The answer goes out as `crewmate_answer` (Act scope); the
/// card closes by itself when the task's push brings the answered ask.
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/require_mux.dart';
import 'package:atlas_mobile/core/protocol/ask_request.dart';
import 'package:atlas_mobile/core/protocol/crew_task.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/features/crew/chat/crewmate_card_frame.dart';
import 'package:atlas_mobile/features/crew/crew_provider.dart';
import 'package:atlas_mobile/features/crew/widgets/pill_buttons.dart';

class TaskAskCard extends ConsumerStatefulWidget {
  const TaskAskCard({
    super.key,
    required this.task,
    required this.device,
    required this.crewmateId,
  });

  final CrewmateTask task;
  final MacDevice device;
  final String crewmateId;

  @override
  ConsumerState<TaskAskCard> createState() => _TaskAskCardState();
}

class _TaskAskCardState extends ConsumerState<TaskAskCard> {
  final _controller = TextEditingController();
  bool _sending = false;
  String? _error;

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  Future<void> _answer({String? key, String? text}) async {
    final ask = widget.task.ask;
    if (ask == null || _sending) return;
    setState(() {
      _sending = true;
      _error = null;
    });
    try {
      final mux = await requireMuxOf(ref, widget.device);
      final resp = await mux.call(
        'crewmate_answer',
        ask.answerArgs(widget.crewmateId, key: key, text: text),
      );
      if (resp['ok'] != true) {
        throw resp['error'] as String? ?? 'the server refused';
      }
      _controller.clear();
      ref.invalidate(crewmateTasksProvider((widget.device, widget.crewmateId)));
    } on Object catch (e) {
      // The card stays, with what the server said: the agent is still waiting.
      if (mounted) setState(() => _error = errorText(e));
    } finally {
      if (mounted) setState(() => _sending = false);
    }
  }

  void _sendText() {
    final v = _controller.text.trim();
    if (v.isNotEmpty) _answer(text: v);
  }

  @override
  Widget build(BuildContext context) {
    final ask = widget.task.ask;
    if (ask == null || !ask.isOpen) return const SizedBox.shrink();
    final c = AtlasColors.of(context);
    return CrewmateCardFrame(
      children: [
        Text(
          ask.question,
          style: Theme.of(context).textTheme.bodyMedium!
              .copyWith(fontWeight: FontWeight.w600, color: c.text),
        ),
        for (final o in ask.options) ...[
          const SizedBox(height: Space.sm),
          _option(c, o),
        ],
        if (ask.allowText) ...[
          const SizedBox(height: Space.md),
          AnswerField(
            controller: _controller,
            hint: 'Or type an answer',
            onSubmitted: (_) => _sendText(),
          ),
          const SizedBox(height: Space.sm),
          WarnPillButton(label: 'Send', onPressed: _sending ? null : _sendText),
        ],
        if (_error != null) ...[
          const SizedBox(height: Space.sm),
          CardError(_error!),
        ],
      ],
    );
  }

  Widget _option(AtlasColors c, AskOption o) => InkWell(
    borderRadius: BorderRadius.circular(10),
    onTap: _sending ? null : () => _answer(key: o.key),
    child: Container(
      width: double.infinity,
      padding: const EdgeInsets.symmetric(
        horizontal: Space.md,
        vertical: Space.md,
      ),
      decoration: BoxDecoration(
        border: Border.all(color: c.borderSubtle),
        borderRadius: BorderRadius.circular(10),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            o.title,
            style: Theme.of(context).textTheme.bodyLarge!
                .copyWith(color: c.text),
          ),
          if (o.subtitle != null)
            Text(
              o.subtitle!,
              style: Theme.of(context).textTheme.bodySmall!
                  .copyWith(color: c.muted),
            ),
        ],
      ),
    ),
  );
}
