/// `crew_voice_status`: the part of the crew's voice settings the phone needs. The phone recognises speech on
/// itself (`core/voice/`) and never sends audio to hostd, so it reads one thing: whether the crew allows
/// dictation (`enabled`, the owner's crew-wide switch). Everything else in the reply (engine, languages,
/// replacements) is for the desktop and Telegram, which recognise on the crew server, and is ignored here.
/// The shape is hostd's: `test/crew_voice_contract_test.dart` reads its real reply as a fixture. Plain Dart.
library;

class CrewVoice {
  const CrewVoice({required this.enabled});

  /// The crew-wide switch. A hostd that does not know the command (or does not send the field) is "off": the
  /// mic stays hidden rather than appearing on a crew that never agreed to voice.
  final bool enabled;

  static const off = CrewVoice(enabled: false);

  factory CrewVoice.fromJson(Map<String, dynamic> j) =>
      CrewVoice(enabled: j['enabled'] == true);
}
