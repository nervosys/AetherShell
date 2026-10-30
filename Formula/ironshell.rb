# Homebrew Formula for IronShell
# This formula lives in the IronShell repository itself, not a homebrew-tap
# repository, so tap it by URL:
#   brew tap nervosys/aethershell https://github.com/nervosys/AetherShell
#   brew install nervosys/aethershell/ironshell

class Ironshell < Formula
  desc "AI-powered typed shell with functional pipelines and multi-agent support"
  homepage "https://github.com/nervosys/AetherShell"
  url "https://github.com/nervosys/AetherShell/archive/refs/tags/v12.0.2.tar.gz"
  sha256 "77c274507784bcf89b576fbcfb0f649242f85292b3ee5202604506ddda247e4e"
  license "AGPL-3.0-or-later"
  head "https://github.com/nervosys/AetherShell.git", branch: "master"

  depends_on "rust" => :build

  def install
    system "cargo", "build", "--release", "--features", "native"
    bin.install "target/release/ish"
    bin.install "target/release/ae"
    bin.install "target/release/aimodel"

    # No completions are generated: `ish` has no `completions` subcommand.
    # This previously called generate_completions_from_executable(...), which
    # aborts the install when the command does not exist.

    # Install documentation
    doc.install "README.md"
    doc.install "docs/SPEC.md" if File.exist?("docs/SPEC.md")
  end

  def caveats
    <<~EOS
      IronShell has been installed!

      To get started:
        ish                   # Start interactive REPL
        ish script.ish        # Run a script
        ish tui               # Launch TUI mode

      For AI features, set your API key:
        export OPENAI_API_KEY=your-key-here
        # Or use local models with Ollama

      Documentation: https://github.com/nervosys/AetherShell
    EOS
  end

  test do
    # Test basic evaluation
    assert_equal "3", shell_output("#{bin}/ish -c '1 + 2'").strip

    # Test version
    assert_match version.to_s, shell_output("#{bin}/ish --version")

    # Canonical, byte-stable JSON. `--json` is not a top-level flag, so the
    # previous assertion here failed for anyone who ran `brew test`.
    output = shell_output("#{bin}/ish --deterministic -c '[1, 2, 3]'")
    assert_match "[1,2,3]", output
  end
end
