class OpencodeSessionMemorySidebarInstaller < Formula
  desc "Installer for the OpenCode Session Memory sidebar plugin"
  homepage "https://github.com/Topabaem05/CancerBroker"
  url "https://github.com/Topabaem05/CancerBroker/releases/download/CancerBroker-v0.1.2/CancerBroker.cjs"
  version "0.1.2"
  sha256 "f9078d788be55dc94d128fd2b0be9dfc4bf5b7184e39b59591b2e338e7b15c8c"
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
    assert_match "opencode-session-memory-sidebar", config_path.read

    system bin/"opencode-session-memory-sidebar-installer", "uninstall", "--config", config_path
    refute_match "opencode-session-memory-sidebar", config_path.read
  end
end
