/// The crew server's GitHub login (`crew_github_status` / `crew_github_login`).
library;

/// GitHub status on a crew server (`crew_github_status`).
class CrewGithubStatus {
  const CrewGithubStatus({required this.installed, required this.login});

  final bool installed;
  final String? login;

  factory CrewGithubStatus.fromJson(Map<String, dynamic> j) => CrewGithubStatus(
    installed: j['installed'] as bool? ?? false,
    login: j['login'] as String?,
  );
}

/// Active device-flow login for a crew server (`crew_github_login_start`).
class CrewGithubLogin {
  const CrewGithubLogin({
    required this.userCode,
    required this.verificationUri,
  });

  final String userCode;
  final String verificationUri;

  factory CrewGithubLogin.fromJson(Map<String, dynamic> j) => CrewGithubLogin(
    userCode: j['user_code'] as String? ?? '',
    verificationUri: j['verification_uri'] as String? ?? '',
  );
}
