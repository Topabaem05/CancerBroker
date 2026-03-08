class OpencodeSessionMemorySidebarInstaller < Formula
  desc "Installer for the OpenCode Session Memory sidebar plugin"
  homepage "https://github.com/Topabaem05/CancerBroker"
  url "https://github.com/Topabaem05/CancerBroker/releases/download/opencode-session-memory-sidebar-installer-v0.1.0/opencode-session-memory-sidebar-installer.cjs"
  version "0.1.0"
  sha256 "3d99103d356ad726e0399c901904b073d2fd290901e6e29c1032cf41e45adf9a"
  license "MIT"

  depends_on "node"

  def install
    libexec.install "opencode-session-memory-sidebar-installer.cjs"

    (libexec/"opencode-session-memory-sidebar-installer").write <<~SH
      #!/bin/sh
      exec "#{Formula["node"].opt_bin}/node" "#{libexec}/opencode-session-memory-sidebar-installer.cjs" "$@"
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
