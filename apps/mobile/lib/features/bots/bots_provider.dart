import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_riverpod/legacy.dart';
import 'package:atlas_mobile/core/protocol/models.dart';
import 'package:atlas_mobile/core/connection/connection_notifier.dart';
import 'package:atlas_mobile/core/providers/push_events_provider.dart';

// Result of bot_list: whether the server is a bot server, and its bots.
typedef BotListResult = ({bool isBotServer, String hostName, List<BotListEntry> bots});

/// Loads bots from a device. Returns isBotServer=false when bot_list errors
/// with the 'not a bot server' message (hostd without ATLAS_BOT_HOST=1).
final botsProvider = AsyncNotifierProvider.autoDispose
    .family<BotsNotifier, BotListResult, MacDevice>(BotsNotifier.new);

class BotsNotifier extends AsyncNotifier<BotListResult> {
  BotsNotifier(this.device);
  final MacDevice device;

  @override
  Future<BotListResult> build() async {
    // Re-fetch on push events for this device.
    ref.listen(devicePushEventsProvider(device), (_, ev) {
      if (!ev.hasValue) return;
      final e = ev.value!;
      final evName = e['ev'] as String? ?? '';
      if (evName == 'bot_task' || evName.startsWith('agent_status')) {
        ref.invalidateSelf();
      }
    });
    final mux = ref.watch(connectionNotifierProvider(device)).value;
    if (mux == null) {
      return (isBotServer: false, hostName: device.name, bots: const <BotListEntry>[]);
    }
    final resp = await mux.call('bot_list', {});
    if (resp['ok'] != true) {
      return (isBotServer: false, hostName: device.name, bots: const <BotListEntry>[]);
    }
    final data = (resp['data'] as Map).cast<String, dynamic>();
    final hostName = data['host_name'] as String? ?? device.name;
    final bots = [
      for (final b in (data['bots'] as List<dynamic>?) ?? const [])
        BotListEntry.fromJson((b as Map).cast()),
    ];
    return (isBotServer: true, hostName: hostName, bots: bots);
  }
}

/// Loads tasks for a single bot.
final botTasksProvider = FutureProvider.autoDispose
    .family<List<BotTask>, (MacDevice, String)>((ref, arg) async {
  final (device, botId) = arg;
  // Refresh on bot_task pushes.
  ref.listen(devicePushEventsProvider(device), (_, ev) {
    if (!ev.hasValue) return;
    final e = ev.value!;
    final bt = e['bot'] as Map?;
    if ((e['ev'] as String?) == 'bot_task' && bt != null && bt['id'] == botId) {
      ref.invalidateSelf();
    }
  });
  final mux = ref.watch(connectionNotifierProvider(device)).value;
  if (mux == null) return const [];
  final resp = await mux.call('bot_tasks', {'id': botId, 'limit': 50});
  if (resp['ok'] != true) return const [];
  final data = (resp['data'] as Map).cast<String, dynamic>();
  return [
    for (final t in (data['tasks'] as List<dynamic>?) ?? const [])
      BotTask.fromJson((t as Map).cast()),
  ];
});

/// Accumulates pending computer invites from push events and on-connect fetch.
final computerInvitesProvider =
    StateNotifierProvider<ComputerInvitesNotifier, List<ComputerInvite>>(
      (_) => ComputerInvitesNotifier(),
    );

class ComputerInvitesNotifier extends StateNotifier<List<ComputerInvite>> {
  ComputerInvitesNotifier() : super(const []);

  void add(ComputerInvite invite) {
    if (state.any((i) => i.url == invite.url)) return;
    state = [...state, invite];
  }

  void remove(String url) {
    state = [for (final i in state) if (i.url != url) i];
  }
}
