class Atlas < Formula
  desc "Native development environment orchestrator with TUI"
  homepage "https://github.com/codeatlasdev/atlas"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/codeatlasdev/atlas/releases/download/v#{version}/atlas-#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "d5a57be8c3b53797e2875aa1b3bef1e33ccf4e1023df7c9b3e01b4d99a28f2e3"
    end
  end

  def install
    bin.install "atlas"
  end

  def caveats
    <<~EOS
      To start the development TUI:
        atlas dev

      To update:
        atlas self-update
    EOS
  end

  test do
    assert_match "atlas", shell_output("#{bin}/atlas --help 2>&1")
  end
end
