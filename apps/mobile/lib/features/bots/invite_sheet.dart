import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';

import 'package:atlas_mobile/features/inbox/inbox.dart';
import 'package:atlas_mobile/features/pair/pair_screen.dart';

// ── sheet ─────────────────────────────────────────────────────────────────────

class InviteSheet extends ConsumerStatefulWidget {
  const InviteSheet({
    super.key,
    required this.invite,
    required this.fromDevice,
    required this.onDismiss,
  });

  final ComputerInvite invite;

  /// The Mac that sent the computer_invite push; used to call
  /// mobile_invite_dismiss so the Mac stops showing the QR.
  final MacDevice fromDevice;

  final VoidCallback onDismiss;

  @override
  ConsumerState<InviteSheet> createState() => _InviteSheetState();
}

class _InviteSheetState extends ConsumerState<InviteSheet> {
  bool _pairing = false;
  String? _error;

  Future<void> _dismiss() async {
    // Tell the Mac we dismissed so it stops waiting.
    final mux =
        ref.read(connectionNotifierProvider(widget.fromDevice)).value;
    if (mux != null) {
      await mux.call(
        'mobile_invite_dismiss',
        {'url': widget.invite.url},
      );
    }
    if (mounted) Navigator.of(context).pop();
    widget.onDismiss();
  }

  Future<void> _add() async {
    setState(() {
      _pairing = true;
      _error = null;
    });

    // Pair using the invite URL (atlas-mobile://pair?...).
    await ref
        .read(pairNotifierProvider.notifier)
        .pair(widget.invite.url);

    if (!mounted) return;
    final pairState = ref.read(pairNotifierProvider);

    if (pairState.status == PairStatus.success) {
      // Dismiss on the Mac.
      final mux =
          ref.read(connectionNotifierProvider(widget.fromDevice)).value;
      if (mux != null) {
        await mux.call(
          'mobile_invite_dismiss',
          {'url': widget.invite.url},
        );
      }
      if (!mounted) return;
      // Refresh paired devices so the new server shows up immediately.
      ref.invalidate(pairedDevicesProvider);
      Navigator.of(context).pop();
      widget.onDismiss();
    } else {
      setState(() {
        _pairing = false;
        _error = pairState.error ?? 'Pairing failed';
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    final mq = MediaQuery.of(context);

    return Padding(
      padding: EdgeInsets.only(bottom: mq.viewInsets.bottom),
      child: SafeArea(
        child: Padding(
          padding: const EdgeInsets.fromLTRB(
            Space.xl,
            Space.lg,
            Space.xl,
            Space.xl,
          ),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              // ── drag handle ─────────────────────────────────────────────
              Container(
                width: 36,
                height: 4,
                margin: const EdgeInsets.only(bottom: Space.xl),
                decoration: BoxDecoration(
                  color: c.border,
                  borderRadius: BorderRadius.circular(2),
                ),
              ),

              // ── icon ────────────────────────────────────────────────────
              Row(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  Icon(Icons.laptop_mac, size: 36, color: c.accent),
                  Padding(
                    padding: const EdgeInsets.symmetric(
                      horizontal: Space.md,
                    ),
                    child: Icon(
                      Icons.arrow_forward,
                      size: 20,
                      color: c.faint,
                    ),
                  ),
                  Icon(Icons.phone_iphone, size: 36, color: c.accent),
                ],
              ),
              const SizedBox(height: Space.xl),

              // ── title ────────────────────────────────────────────────────
              Text(
                'New bot server',
                style: Theme.of(context).textTheme.titleMedium,
                textAlign: TextAlign.center,
              ),
              const SizedBox(height: Space.sm),

              // ── body ─────────────────────────────────────────────────────
              Text(
                '${widget.invite.name} was added on your Mac. '
                'Add it to your phone? Its bots will work even with your Mac off.',
                style: TextStyle(color: c.muted, height: 1.5),
                textAlign: TextAlign.center,
              ),
              const SizedBox(height: Space.xl),

              // ── error ─────────────────────────────────────────────────────
              if (_error != null) ...[
                Text(
                  _error!,
                  style: TextStyle(color: c.error, fontSize: 13),
                  textAlign: TextAlign.center,
                ),
                const SizedBox(height: Space.sm),
              ],

              // ── buttons ──────────────────────────────────────────────────
              Row(
                children: [
                  Expanded(
                    child: OutlinedButton(
                      onPressed: _pairing ? null : _dismiss,
                      child: const Text('Not now'),
                    ),
                  ),
                  const SizedBox(width: Space.md),
                  Expanded(
                    child: FilledButton(
                      onPressed: _pairing ? null : _add,
                      child: _pairing
                          ? const SizedBox.square(
                              dimension: 18,
                              child: CircularProgressIndicator(
                                strokeWidth: 2,
                              ),
                            )
                          : const Text('Add'),
                    ),
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }
}
