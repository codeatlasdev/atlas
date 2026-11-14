/// The crew-wide settings of one crew server: whether long replies are summarized, and by which model.
/// The desktop's Settings → Crew; saved with the `act` scope, so a read-only phone sees it but cannot change it.
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/protocol/crew_ops_models.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/crew/ops/crew_ops_provider.dart';

class CrewSettingsScreen extends ConsumerWidget {
  const CrewSettingsScreen({super.key, required this.device});
  final MacDevice device;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final settings = ref.watch(crewSettingsProvider(device));
    return Scaffold(
      appBar: AppBar(title: const Text('Crew settings')),
      body: settings.when(
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (e, _) => EmptyState(
          title: "Couldn't read the crew settings",
          message: '${e is StateError ? e.message : e}',
          action: OutlinedButton(
            onPressed: () => ref.invalidate(crewSettingsProvider(device)),
            child: const Text('Try again'),
          ),
        ),
        data: (s) => _Form(device: device, initial: s),
      ),
    );
  }
}

class _Form extends ConsumerStatefulWidget {
  const _Form({required this.device, required this.initial});
  final MacDevice device;
  final CrewSettings initial;

  @override
  ConsumerState<_Form> createState() => _FormState();
}

class _FormState extends ConsumerState<_Form> {
  late bool _short = widget.initial.shortReplies;
  late final _model = TextEditingController(
    text: widget.initial.shortRepliesModel ?? '',
  );
  bool _busy = false;
  String? _status;

  @override
  void dispose() {
    _model.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    setState(() {
      _busy = true;
      _status = null;
    });
    try {
      await saveCrewSettings(
        ref,
        widget.device,
        CrewSettings(
          shortReplies: _short,
          shortRepliesModel: _model.text.trim(),
        ),
      );
      if (mounted) setState(() => _status = 'Saved');
    } catch (e) {
      if (mounted) {
        setState(
          () => _status = "Couldn't save: ${e is StateError ? e.message : e}",
        );
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final c = AtlasColors.of(context);
    return ListView(
      padding: const EdgeInsets.all(Space.lg),
      children: [
        AtlasCard(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  const Expanded(
                    child: Text(
                      'Short replies',
                      style: TextStyle(
                        fontSize: 15,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ),
                  Switch(
                    value: _short,
                    onChanged: _busy
                        ? null
                        : (v) {
                            setState(() => _short = v);
                            _save();
                          },
                  ),
                ],
              ),
              Text(
                'Summarize long crewmate replies with a cheap model. A crewmate can override this on its own profile.',
                style: TextStyle(color: c.muted, fontSize: 13, height: 1.4),
              ),
            ],
          ),
        ),
        AtlasCard(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Text(
                'Summary model',
                style: TextStyle(fontSize: 15, fontWeight: FontWeight.w600),
              ),
              const SizedBox(height: Space.sm),
              TextField(
                key: const Key('crew-model'),
                controller: _model,
                autocorrect: false,
                enableSuggestions: false,
                decoration: const InputDecoration(
                  hintText: 'Empty uses the server default',
                ),
              ),
            ],
          ),
        ),
        const SizedBox(height: Space.sm),
        Row(
          children: [
            Expanded(
              child: Text(
                _status ?? '',
                style: TextStyle(
                  color: (_status ?? '').startsWith('Saved')
                      ? c.success
                      : c.error,
                  fontSize: 13,
                ),
              ),
            ),
            FilledButton(
              onPressed: _busy ? null : _save,
              child: const Text('Save'),
            ),
          ],
        ),
      ],
    );
  }
}
