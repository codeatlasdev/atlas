class Atlas < Formula
  desc "Native development environment orchestrator with TUI"
  homepage "https://github.com/codeatlasdev/atlas"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/codeatlasdev/atlas/releases/download/v#{version}/atlas-#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER"
    end
    on_intel do
      url "https://github.com/codeatlasdev/atlas/releases/download/v#{version}/atlas-#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER"
    end
  end

  def install
    bin.install "atlas"
    bin.install "atlas-daemon"

    # Generate shell completions
    generate_completions_from_executable(bin/"atlas", "completions")
  end

  service do
    run [opt_bin/"atlas-daemon"]
    keep_alive true
    log_path var/"log/atlas-daemon.log"
    error_log_path var/"log/atlas-daemon.log"
    working_dir HOMEBREW_PREFIX
  end

  def caveats
    <<~EOS
      To start the Atlas daemon:
        brew services start atlas

      To start the development TUI:
        atlas dev
    EOS
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/atlas --version 2>&1", 1)
  end
end
