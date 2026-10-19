import 'dart:convert';

import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:atlas_mobile/core/protocol/models.dart';

/// Secure-storage key for the list of paired Mac devices.
const _kDevicesKey = 'atlas_mobile.devices';

/// Secure-storage key for the phone's own iroh secret key (hex-encoded).
const _kSecretKeyKey = 'atlas_mobile.secret_key';

/// Read/write paired [MacDevice]s to [FlutterSecureStorage].
class MacStorage {
  MacStorage({FlutterSecureStorage? storage})
    : _storage = storage ?? const FlutterSecureStorage();

  final FlutterSecureStorage _storage;

  /// Load all paired devices, or `[]` if none stored yet.
  Future<List<MacDevice>> loadDevices() async {
    final raw = await _storage.read(key: _kDevicesKey);
    if (raw == null || raw.isEmpty) return [];
    final list = jsonDecode(raw) as List<dynamic>;
    return list
        .map((e) => MacDevice.fromJson(e as Map<String, dynamic>))
        .toList();
  }

  /// Persist the full device list.
  Future<void> saveDevices(List<MacDevice> devices) async {
    final json = jsonEncode(devices.map((d) => d.toJson()).toList());
    await _storage.write(key: _kDevicesKey, value: json);
  }

  /// Add or update (by nodeId) a device, then persist.
  Future<void> upsertDevice(MacDevice device) async {
    final devices = await loadDevices();
    final idx = devices.indexWhere((d) => d.nodeId == device.nodeId);
    if (idx == -1) {
      devices.add(device);
    } else {
      devices[idx] = device;
    }
    await saveDevices(devices);
  }

  /// Remove a device by nodeId.
  Future<void> removeDevice(String nodeId) async {
    final devices = await loadDevices();
    devices.removeWhere((d) => d.nodeId == nodeId);
    await saveDevices(devices);
  }

  // ── Secret key ────────────────────────────────────────────────────────────

  /// Load the phone's own secret key hex, or `null` if not set yet.
  Future<String?> loadSecretKey() async => _storage.read(key: _kSecretKeyKey);

  /// Persist the phone's own secret key hex.
  Future<void> saveSecretKey(String hexKey) async =>
      _storage.write(key: _kSecretKeyKey, value: hexKey);
}
