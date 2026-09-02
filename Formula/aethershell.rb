# Homebrew Formula for AetherShell
# To install: brew tap nervosys/tap && brew install aethershell
# Or: brew install nervosys/tap/aethershell

class Aethershell < Formula
  desc "AI-powered typed shell with functional pipelines and multi-agent support"
  homepage "https://github.com/nervosys/AetherShell"
  url "https://github.com/nervosys/AetherShell/archive/refs/tags/v12.0.1.tar.gz"
  sha256 "2e39a24c49836cd83fa7a272b07c23cb939996ec9370718831fd1b3e5a9a3a30"
  license "AGPL-3.0-or-later"
  head "https://github.com/nervosys/AetherShell.git", branch: "master"

  depends_on "rust" => :build

  def install
    system "cargo", "build", "--release", "--features", "native"
    bin.install "target/release/ae"
    bin.install "target/release/aimodel"

    # No completions are generated: `ae` has no `completions` subcommand.
    # This previously called generate_completions_from_executable(...), which
    # aborts the install when the command does not exist.

    # Install documentation
    doc.install "README.md"
    doc.install "docs/SPEC.md" if File.exist?("docs/SPEC.md")
  end

  def caveats
    <<~EOS
      AetherShell has been installed!

      To get started:
        ae                    # Start interactive REPL
        ae script.ae          # Run a script
        ae --tui              # Launch TUI mode

      For AI features, set your API key:
        export OPENAI_API_KEY=your-key-here
        # Or use local models with Ollama

      Documentation: https://github.com/nervosys/AetherShell
    EOS
  end

  test do
    # Test basic evaluation
    assert_equal "3", shell_output("#{bin}/ae -c '1 + 2'").strip

    # Test version
    assert_match version.to_s, shell_output("#{bin}/ae --version")

    # Canonical, byte-stable JSON. `--json` is not a top-level flag, so the
    # previous assertion here failed for anyone who ran `brew test`.
    output = shell_output("#{bin}/ae --deterministic -c '[1, 2, 3]'")
    assert_match "[1,2,3]", output
  end
end
