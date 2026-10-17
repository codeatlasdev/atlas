/// Abstract Transport interface.
///
/// Implementations:
/// - [FrbTransport] — iroh P2P over the native Rust crate (production).
/// - [FakeTransport] — in-memory stub for unit tests.
library;

/// A bidirectional text-line transport.
///
/// Lines are expected to be valid NDJSON (no embedded newlines).
abstract class Transport {
  /// Stream of lines received from the remote end.
  ///
  /// The stream is a broadcast stream: multiple listeners receive the same
  /// events.  Closes when the underlying connection is lost.
  Stream<String> get incomingLines;

  /// Send [line] to the remote end.
  ///
  /// The implementation MUST append a newline if the protocol requires one.
  /// Throws [StateError] if the transport is already closed.
  Future<void> sendLine(String line);

  /// Close the connection and release resources.
  Future<void> close();

  /// Whether the transport is currently open.
  bool get isOpen;
}
