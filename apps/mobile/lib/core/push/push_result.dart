/// The outcome of one step of getting push to work (the relay's grant, the host's capability, the
/// host's `push_register`): a value, or the one-line reason it failed, written for the user.
///
/// Every step used to answer `null` or `false` on any error, so nobody could tell why
/// notifications did not arrive.
sealed class PushResult<T> {
  const PushResult();
}

final class PushOk<T> extends PushResult<T> {
  const PushOk(this.value);
  final T value;
}

final class PushFail<T> extends PushResult<T> {
  const PushFail(this.reason);

  /// For the user: what went wrong and, when there is one, what to do.
  final String reason;
}
