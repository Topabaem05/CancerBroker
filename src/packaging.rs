pub fn render_systemd_unit(exec_path: &str, config_path: &str, workdir: &str) -> String {
    SYSTEMD_TEMPLATE
        .replace("{{EXEC_PATH}}", exec_path)
        .replace("{{CONFIG_PATH}}", config_path)
        .replace("{{WORKDIR}}", workdir)
}

pub fn render_launchd_plist(exec_path: &str, config_path: &str, log_path: &str) -> String {
    LAUNCHD_TEMPLATE
        .replace("{{EXEC_PATH}}", exec_path)
        .replace("{{CONFIG_PATH}}", config_path)
        .replace("{{LOG_PATH}}", log_path)
}

const SYSTEMD_TEMPLATE: &str = r#"[Unit]
Description=opencode-guardian sidecar
After=network.target

[Service]
Type=simple
WorkingDirectory={{WORKDIR}}
ExecStart={{EXEC_PATH}} --config {{CONFIG_PATH}} run-once --json
Restart=on-failure

[Install]
WantedBy=multi-user.target
"#;

const LAUNCHD_TEMPLATE: &str = r#"<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">
<plist version=\"1.0\">
  <dict>
    <key>Label</key>
    <string>com.opencode.guardian</string>
    <key>ProgramArguments</key>
    <array>
      <string>{{EXEC_PATH}}</string>
      <string>--config</string>
      <string>{{CONFIG_PATH}}</string>
      <string>run-once</string>
      <string>--json</string>
    </array>
    <key>StandardOutPath</key>
    <string>{{LOG_PATH}}</string>
    <key>StandardErrorPath</key>
    <string>{{LOG_PATH}}</string>
    <key>RunAtLoad</key>
    <true/>
  </dict>
</plist>
"#;
