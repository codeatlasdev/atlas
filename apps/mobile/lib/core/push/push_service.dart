import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:firebase_core/firebase_core.dart';
import 'package:firebase_messaging/firebase_messaging.dart';
import 'package:flutter_riverpod/legacy.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:atlas_mobile/core/protocol/multiplexer.dart';

/// Provider that exposes the current FCM token, or null when push is unavailable.
final fcmTokenProvider = StateProvider<String?>((ref) => null);

/// Manages Firebase Messaging registration.
///
/// Call [PushService.init] once from [main] (after RustLib.init()).
/// When google-services.json / GoogleService-Info.plist are absent (local
/// development without Firebase), this is a no-op: the app builds and runs
/// normally, push simply does not work.
class PushService {
  PushService._();

  static bool _available = false;

  static bool get available => _available;

  /// Initialise Firebase and request notification permission.
  ///
  /// Safe to call when Firebase config files are absent: any exception is caught
  /// and push is silently disabled.
  static Future<void> init(WidgetRef? ref) async {
    try {
      await Firebase.initializeApp();
      _available = true;
      final messaging = FirebaseMessaging.instance;
      await messaging.requestPermission(alert: true, badge: true, sound: true);
      // firebase_messaging 17 deprecates getToken/onTokenRefresh for register()/onRegistered,
      // which yield a Firebase Installation ID, not the FCM registration token that
      // atlas-host's push sender (bots/push.rs, FCM HTTP v1 `token`) targets. Moving needs
      // the sender changed and real FCM delivery verified together.
      // ignore: deprecated_member_use
      final token = await messaging.getToken();
      if (token != null && ref != null && ref.context.mounted) {
        ref.read(fcmTokenProvider.notifier).state = token;
      }
      // ignore: deprecated_member_use
      messaging.onTokenRefresh.listen((newToken) {
        if (ref != null && ref.context.mounted) {
          ref.read(fcmTokenProvider.notifier).state = newToken;
        }
      });
    } catch (e) {
      debugPrint('atlas-mobile: push unavailable (Firebase not configured): $e');
      _available = false;
    }
  }

  /// Register the current FCM token with a bot computer.
  static Future<void> registerWithComputer(
    Multiplexer mux,
    String token,
  ) async {
    try {
      await mux.call('push_register', {'token': token, 'platform': 'fcm'});
    } catch (e) {
      debugPrint('atlas-mobile: push_register failed: $e');
    }
  }

  /// Unregister a FCM token from a bot computer.
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
