import 'dart:async';

import 'package:firebase_core/firebase_core.dart';
import 'package:firebase_messaging/firebase_messaging.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/legacy.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/permissions/permissions.dart';
import 'package:atlas_mobile/core/protocol/multiplexer.dart';
import 'package:atlas_mobile/core/push/push_payload.dart';
import 'package:atlas_mobile/core/push/push_tap.dart';

/// Provider that exposes the current FCM token, or null when push is unavailable (no Firebase config in
/// this build) or the person has not allowed notifications.
final fcmTokenProvider = StateProvider<String?>((ref) => null);

/// What push needs from Firebase, apart from the permission (that is `PermissionsPort`): tests replace it, so
/// the startup and token flow run with no plugin.
abstract interface class PushBackend {
  /// Initialises Firebase; false when this build has no Firebase config (local development).
  Future<bool> start();

  Future<String?> token();
  Stream<String> get tokenRefresh;

  /// The data of a push the person tapped while the app was in the background.
  Stream<Map<String, dynamic>> get opened;

  /// The data of the push whose tap launched the app, if one did.
  Future<Map<String, dynamic>?> launchedBy();
}

class FirebasePushBackend implements PushBackend {
  @override
  Future<bool> start() async {
    try {
      await Firebase.initializeApp();
      return true;
    } catch (e) {
      debugPrint(
        'atlas-mobile: push unavailable (Firebase not configured): $e',
      );
      return false;
    }
  }

  // firebase_messaging 17 deprecates getToken/onTokenRefresh for register()/onRegistered,
  // which yield a Firebase Installation ID, not the FCM registration token that
  // atlas-host's push sender (crew/push.rs, FCM HTTP v1 `token`) targets. Moving needs
  // the sender changed and real FCM delivery verified together.
  @override
  // ignore: deprecated_member_use
  Future<String?> token() => FirebaseMessaging.instance.getToken();

  @override
  // ignore: deprecated_member_use
  Stream<String> get tokenRefresh => FirebaseMessaging.instance.onTokenRefresh;

  @override
  Stream<Map<String, dynamic>> get opened =>
      FirebaseMessaging.onMessageOpenedApp.map((m) => m.data);

  @override
  Future<Map<String, dynamic>?> launchedBy() async =>
      (await FirebaseMessaging.instance.getInitialMessage())?.data;
}

final pushBackendProvider = Provider<PushBackend>(
  (ref) => FirebasePushBackend(),
);

/// Manages Firebase Messaging registration.
///
/// Call [PushService.init] once from the app root. It never shows the system notification prompt: the person
/// is asked in context (after the first pairing, or from Settings → This phone, `askPermission`), and the FCM
/// token is fetched, and so registered with the computers, only once notifications are allowed. When
/// google-services.json / GoogleService-Info.plist are absent (local development without Firebase), this is a
/// no-op: the app builds and runs normally, push simply does not work.
class PushService {
  PushService._();

  static AppLifecycleListener? _life;

  /// Starts Firebase, listens for taps on pushes, and follows the notification permission: a token once it
  /// is granted (at launch, after the prompt, or after the person turned it on in the system Settings).
  static Future<void> init(WidgetRef ref) async {
    final backend = ref.read(pushBackendProvider);
    if (!await backend.start()) return;
    // A tap on a push opens what it is about: while the app was in the background, or the tap
    // that launched it. Unknown payloads (a newer hostd) are ignored by `PushPayload.fromData`.
    void tapped(Map<String, dynamic> data) {
      final p = PushPayload.fromData(data);
      if (p != null && ref.context.mounted) {
        ref.read(pushTapProvider.notifier).state = p;
      }
    }

    backend.opened.listen(tapped);
    final launch = await backend.launchedBy();
    if (launch != null) tapped(launch);

    // Publishes the FCM token and keeps it fresh, once per start: later grants only re-enter here.
    var tracking = false;
    Future<void> track() async {
      if (tracking) return;
      tracking = true;
      try {
        final token = await backend.token();
        if (token != null && ref.context.mounted) {
          ref.read(fcmTokenProvider.notifier).state = token;
        }
        backend.tokenRefresh.listen((newToken) {
          if (ref.context.mounted) {
            ref.read(fcmTokenProvider.notifier).state = newToken;
          }
        });
      } catch (e) {
        tracking = false;
        debugPrint('atlas-mobile: push token unavailable: $e');
      }
    }

    final notifications = permissionStateProvider(
      AtlasPermission.notifications,
    );
    ref.listenManual(notifications, (_, s) {
      if (s.value == PermissionState.granted) unawaited(track());
    }, fireImmediately: true);
    // Turned on in the system Settings while the app was in the background: read it again on return.
    _life?.dispose();
    _life = AppLifecycleListener(
      onResume: () {
        if (ref.context.mounted) ref.invalidate(notifications);
      },
    );
  }

  /// Unregister a FCM token from a crew server.
  static Future<void> unregisterFromComputer(
    Multiplexer mux,
    String token,
  ) async {
    try {
      await mux.call('push_unregister', {'token': token});
    } catch (e) {
      debugPrint('atlas-mobile: push_unregister failed: $e');
    }
  }
}
