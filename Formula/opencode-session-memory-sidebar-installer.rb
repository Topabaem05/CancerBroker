class OpencodeSessionMemorySidebarInstaller < Formula
  desc "Installer for the Opencode RAM optimizer tool"
  homepage "https://github.com/Topabaem05/CancerBroker"
  url "https://github.com/Topabaem05/CancerBroker/releases/download/CancerBroker-v0.1.7/CancerBroker.cjs"
  version "0.1.7"
  sha256 "f24bedab661a27f614dbeed3f9bd2c84bacd118a025cbedb55ccd23a2088b035"
  license "MIT"

  depends_on "node"

  def install
    libexec.install "CancerBroker.cjs"

    (libexec/"opencode-session-memory-sidebar-installer").write <<~SH
      #!/bin/sh
      exec "#{Formula["node"].opt_bin}/node" "#{libexec}/CancerBroker.cjs" "$@"
    SH

    bin.install libexec/"opencode-session-memory-sidebar-installer"
  end

  test do
    config_path = testpath/"opencode.json"

    system bin/"opencode-session-memory-sidebar-installer", "--config", config_path
    assert_predicate testpath/"tools/session_memory.js", :exist?

    system bin/"opencode-session-memory-sidebar-installer", "uninstall", "--config", config_path
    refute_predicate testpath/"tools/session_memory.js", :exist?
  end
end
