class OpencodeSessionMemorySidebarInstaller < Formula
  desc "Installer for the OpenCode Session Memory tool"
  homepage "https://github.com/Topabaem05/CancerBroker"
  url "https://github.com/Topabaem05/CancerBroker/releases/download/CancerBroker-v0.1.6/CancerBroker.cjs"
  version "0.1.6"
  sha256 "09631ce13a4fb46ca73d58eb3edbfab3cd213aba5ad7154560f2ba5e77e57de6"
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
