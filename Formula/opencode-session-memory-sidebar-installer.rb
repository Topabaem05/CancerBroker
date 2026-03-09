class OpencodeSessionMemorySidebarInstaller < Formula
  desc "Installer for the OpenCode Session Memory tool"
  homepage "https://github.com/Topabaem05/CancerBroker"
  url "https://github.com/Topabaem05/CancerBroker/releases/download/CancerBroker-v0.1.4/CancerBroker.cjs"
  version "0.1.4"
  sha256 "8adb56548ee5cb9f774d7fca8bfe95daa3a2ca58d9e44973b8ab6709ce4dc83f"
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
