import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/providers/push_events_provider.dart';

/// Tracks bots for [device]. Refreshes on bot_task push events.
/// Returns [] if the computer has no bots cap (bot_list fails or returns no bots key).
final botsProvider = AsyncNotifierProvider.autoDispose
    .family<BotsNotifier, List<Bot>, MacDevice>(BotsNotifier.new);

class BotsNotifier extends AsyncNotifier<List<Bot>> {
  BotsNotifier(this.device);
  final MacDevice device;

  @override
  Future<List<Bot>> build() async {
    final mux = await ref.watch(connectionNotifierProvider(device).future);
    if (mux == null) return [];

    final resp = await mux.call('bot_list');
    if (resp['ok'] != true) return [];
    final raw = resp['data']['bots'] as List<dynamic>?;
    if (raw == null) return [];
    final bots = raw.map((e) => Bot.fromJson(e as Map<String, dynamic>)).toList();

    // Keep live via push events.
    ref.listen<AsyncValue<Map<String, dynamic>>>(
      devicePushEventsProvider(device),
      (_, next) {
        next.whenData((event) {
          final ev = event['ev'] as String?;
          if (ev == 'bot_task') {
            // A task state changed: refresh the bot list to get current state.
            ref.invalidateSelf();
          } else if (ev == 'agent_status') {
            // Bot pane agent status changed (approval cleared etc).
            final pane = event['pane_key'] as String? ?? '';
            if (pane.startsWith('bot:')) ref.invalidateSelf();
          }
        });
      },
    );

    return bots;
  }
}

/// Fetches and live-updates the task history for a single bot.
///
/// Refreshes whenever a `bot_task` push event arrives for this bot's id.
final botTasksProvider = AsyncNotifierProvider.autoDispose
    .family<BotTasksNotifier, List<BotTask>, (MacDevice, String)>(
      BotTasksNotifier.new,
    );

class BotTasksNotifier extends AsyncNotifier<List<BotTask>> {
  BotTasksNotifier(this.arg);
  final (MacDevice, String) arg;
  MacDevice get device => arg.$1;
  String get botId => arg.$2;

  @override
  Future<List<BotTask>> build() async {
    final mux = await ref.watch(connectionNotifierProvider(device).future);
    if (mux == null) return [];
    final resp = await mux.call('bot_tasks', {'id': botId, 'limit': 50});
    if (resp['ok'] != true) return [];
    final raw = resp['data']['tasks'] as List<dynamic>? ?? const [];
    final tasks =
        raw.map((e) => BotTask.fromJson(e as Map<String, dynamic>)).toList();

    // Re-fetch when any task on this bot changes state.
    ref.listen<AsyncValue<Map<String, dynamic>>>(
      devicePushEventsProvider(device),
      (_, next) {
        next.whenData((event) {
          if (event['ev'] == 'bot_task' && event['bot'] == botId) {
            ref.invalidateSelf();
          }
        });
      },
    );

    return tasks;
  }
}
