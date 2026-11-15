/// The crew-wide settings of one crew server: whether long replies are summarized, and by which model.
/// The desktop's Settings → Crew; saved with the `act` scope, so a read-only phone sees it but cannot change it.
library;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/protocol/crew_ops_models.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/ui/async_view.dart';
import 'package:atlas_mobile/core/ui/atlas_theme.dart';
import 'package:atlas_mobile/core/ui/components.dart';
import 'package:atlas_mobile/features/crew/ops/crew_ops_provider.dart';
import 'package:atlas_mobile/features/crew/ops/engine_signin.dart';

class CrewSettingsScreen extends ConsumerWidget {
  const CrewSettingsScreen({super.key, required this.device});
  final MacDevice device;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final settings = ref.watch(crewSettingsProvider(device));
    return Scaffold(
      appBar: AppBar(title: const Text('Crew settings')),
      body: AsyncView(
        value: settings,
        errorTitle: "Couldn't read the crew settings",
        onRetry: () => ref.invalidate(crewSettingsProvider(device)),
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
        _EngineSignin(device: widget.device),
        AtlasCard(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Expanded(
                    child: Text(
                      'Short replies',
                      style: Theme.of(context).textTheme.bodyLarge!
                          .copyWith(fontWeight: FontWeight.w600),
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
                style: Theme.of(context).textTheme.bodyMedium!
                    .copyWith(color: c.muted, height: 1.4),
              ),
            ],
          ),
        ),
        AtlasCard(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                'Summary model',
                style: Theme.of(context).textTheme.bodyLarge!
                    .copyWith(fontWeight: FontWeight.w600),
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
                style: Theme.of(context).textTheme.bodyMedium!.copyWith(
                  color: (_status ?? '').startsWith('Saved')
                      ? c.success
                      : c.error,
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

/// One row per engine the crew server can run: whether it is signed in, and Sign in (a terminal on the server that
/// runs only that engine's login command). Needs the `act` scope, like the other writes on this screen.
class _EngineSignin extends ConsumerWidget {
  const _EngineSignin({required this.device});
  final MacDevice device;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final c = AtlasColors.of(context);
    final engines = ref.watch(engineStatusProvider(device));
    return AtlasCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  'Engine sign-in',
                  style: Theme.of(context).textTheme.bodyLarge!
                      .copyWith(fontWeight: FontWeight.w600),
                ),
              ),
              IconButton(
                key: const Key('engines-refresh'),
                tooltip: 'Check again',
                icon: const Icon(Icons.refresh_rounded, size: 20),
                onPressed: () => ref.invalidate(engineStatusProvider(device)),
              ),
            ],
          ),
          Text(
            'The sign-in each crewmate engine uses on this server. When one expires its tasks fail until you sign in again.',
            style: Theme.of(context).textTheme.bodyMedium!
                .copyWith(color: c.muted, height: 1.4),
          ),
          const SizedBox(height: Space.sm),
          engines.when(
            loading: () => const Padding(
              padding: EdgeInsets.all(Space.md),
              child: Center(child: CircularProgressIndicator()),
            ),
            error: (e, _) => Text(
              "Couldn't check the engines: ${e is StateError ? e.message : e}",
              style: Theme.of(context).textTheme.bodyMedium!
                  .copyWith(color: c.error),
            ),
            data: (rows) => Column(
              children: [for (final s in rows) _row(context, ref, c, s)],
            ),
          ),
        ],
      ),
    );
  }

  Widget _row(
    BuildContext context,
    WidgetRef ref,
    AtlasColors c,
    EngineStatus s,
  ) {
    final color = switch (s.state) {
      'signed_in' => c.success,
      'needs_signin' => c.warning,
      _ => c.muted,
    };
    return Padding(
      key: Key('engine-${s.engine}'),
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(s.label, style: Theme.of(context).textTheme.bodyLarge!),
                Text(
                  s.text,
                  style: Theme.of(context).textTheme.bodySmall!
                      .copyWith(color: color),
                ),
              ],
            ),
          ),
          // Not installed: nothing to sign in (install it from the desktop's Crew settings).
          if (s.state != 'not_installed')
            OutlinedButton(
              key: Key('engine-signin-${s.engine}'),
              onPressed: () => openEngineSignin(context, ref, device, s.engine),
              child: Text(s.state == 'signed_in' ? 'Sign in again' : 'Sign in'),
            ),
        ],
      ),
    );
  }
}
