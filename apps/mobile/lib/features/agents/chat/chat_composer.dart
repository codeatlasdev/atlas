import 'package:flutter/material.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';

/// omp queues a typed message as steering while it works; nothing else takes one.
bool canSteer(AgentStatus s) => s.agent == 'omp' && s.state == 'working';

/// The box that sends the next prompt to the agent, with the `/` commands button (omp),
/// a Steer button while omp works, and Stop while any agent works. It holds no state: the
/// screen owns the [controller], the [focusNode] and the sending flags.
///
/// [sub] is the focused subagent's dotted name (`null` on the main agent): only omp
/// relays a line addressed to one, so for any other agent the composer is empty.
class ChatComposer extends StatelessWidget {
  const ChatComposer({
    super.key,
    required this.status,
    required this.sub,
    required this.controller,
    required this.focusNode,
    required this.sending,
    required this.stopping,
    required this.onSend,
    required this.onStop,
    required this.onCommand,
  });

  final AgentStatus status;
  final String? sub;
  final TextEditingController controller;
  final FocusNode focusNode;
  final bool sending;
  final bool stopping;
  final VoidCallback onSend;
  final VoidCallback onStop;
  final VoidCallback onCommand;

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final steer = canSteer(status);
    final canType = canSendChat(status) || steer;
    final sub = this.sub;
    // Only omp relays a line addressed to one of its subagents.
    if (sub != null && status.agent != 'omp') return const SizedBox.shrink();
    final subLabel = sub?.split('.').last;
    final canSend = canType && !sending;
    final working = status.state == 'working';
    OutlineInputBorder border([Color? color]) => OutlineInputBorder(
      borderRadius: BorderRadius.circular(22),
      borderSide: color == null ? BorderSide.none : BorderSide(color: color),
    );
    return Container(
      decoration: BoxDecoration(
        color: c.base,
        border: Border(top: BorderSide(color: c.borderSubtle)),
      ),
      child: SafeArea(
        top: false,
        // The home indicator is under the keyboard when it is up: no inset then.
        bottom: MediaQuery.viewInsetsOf(context).bottom == 0,
        child: Padding(
          padding: const EdgeInsets.fromLTRB(
            Space.xs,
            Space.sm,
            Space.md,
            Space.sm,
          ),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.end,
            children: [
              if (status.agent == 'omp' && sub == null)
                IconButton(
                  tooltip: 'Commands',
                  onPressed: canSendChat(status) && !sending ? onCommand : null,
                  icon: Text(
                    '/',
                    style: Theme.of(context).textTheme.titleLarge!.copyWith(
                      fontFamily: monoFamily,
                      fontFamilyFallback: monoFallback,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                )
              else
                const SizedBox(width: Space.sm),
              Expanded(
                child: TextField(
                  controller: controller,
                  focusNode: focusNode,
                  minLines: 1,
                  maxLines: 5,
                  enabled: canType,
                  decoration: InputDecoration(
                    hintText: sub != null && canType
                        ? (steer
                              ? 'Steer Main about $subLabel…'
                              : 'Message Main about $subLabel')
                        : steer
                        ? 'Steer the agent…'
                        : canType
                        ? 'Message'
                        : switch (status.state) {
                            'working' => 'Working… tap stop to interrupt',
                            'blocked' => 'Answer the request above first',
                            'waiting' =>
                              'The agent asked you something in its terminal',
                            _ => 'The agent cannot take a message now',
                          },
                    fillColor: c.ground,
                    border: border(),
                    enabledBorder: border(),
                    disabledBorder: border(),
                    focusedBorder: border(c.accent),
                  ),
                ),
              ),
              const SizedBox(width: Space.sm),
              if (steer) ...[
                IconButton.filled(
                  tooltip: 'Steer',
                  onPressed: canSend ? onSend : null,
                  icon: sending
                      ? const SizedBox.square(
                          dimension: 18,
                          child: CircularProgressIndicator(strokeWidth: 2),
                        )
                      : const Icon(Icons.send_rounded, size: 18),
                ),
                const SizedBox(width: Space.sm),
              ],
              if (working)
                IconButton.filled(
                  tooltip: 'Stop',
                  style: IconButton.styleFrom(
                    backgroundColor: c.error.withValues(alpha: .2),
                    foregroundColor: c.error,
                  ),
                  onPressed: stopping ? null : onStop,
                  icon: stopping
                      ? const SizedBox.square(
                          dimension: 18,
                          child: CircularProgressIndicator(strokeWidth: 2),
                        )
                      : const Icon(Icons.stop_rounded),
                )
              else
                IconButton.filled(
                  tooltip: 'Send message',
                  onPressed: canSend ? onSend : null,
                  icon: sending
                      ? const SizedBox.square(
                          dimension: 18,
                          child: CircularProgressIndicator(strokeWidth: 2),
                        )
                      : const Icon(Icons.arrow_upward_rounded),
                ),
            ],
          ),
        ),
      ),
    );
  }
}
