fn render_template(template: &str, replacements: &[(&str, &str)]) -> String {
    replacements
        .iter()
        .fold(template.to_string(), |output, (placeholder, value)| {
            output.replace(placeholder, value)
        })
}

pub fn render_systemd_unit(exec_path: &str, config_path: &str, workdir: &str) -> String {
    render_template(
        SYSTEMD_TEMPLATE,
        &[
            ("{{EXEC_PATH}}", exec_path),
            ("{{CONFIG_PATH}}", config_path),
            ("{{WORKDIR}}", workdir),
        ],
    )
}

pub fn render_launchd_plist(exec_path: &str, config_path: &str, log_path: &str) -> String {
    render_template(
        LAUNCHD_TEMPLATE,
        &[
            ("{{EXEC_PATH}}", exec_path),
            ("{{CONFIG_PATH}}", config_path),
            ("{{LOG_PATH}}", log_path),
        ],
    )
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

#[cfg(test)]
mod tests {
    use super::{render_launchd_plist, render_systemd_unit, render_template};

    #[test]
    fn render_template_replaces_all_placeholders() {
        let rendered = render_template(
            "{{A}} {{B}} {{A}}",
            &[("{{A}}", "alpha"), ("{{B}}", "beta")],
        );

        assert_eq!(rendered, "alpha beta alpha");
    }

    #[test]
    fn render_systemd_unit_injects_exec_config_and_workdir() {
        let rendered = render_systemd_unit(
            "/usr/local/bin/opencode-guardian",
            "/etc/opencode-guardian.toml",
            "/var/lib/opencode-guardian",
        );

        assert!(rendered.contains("WorkingDirectory=/var/lib/opencode-guardian"));
        assert!(rendered.contains(
            "ExecStart=/usr/local/bin/opencode-guardian --config /etc/opencode-guardian.toml run-once --json"
        ));
        assert!(!rendered.contains("{{EXEC_PATH}}"));
    }

    #[test]
    fn render_launchd_plist_injects_program_arguments_and_log_path() {
        let rendered = render_launchd_plist(
            "/Applications/opencode-guardian",
            "/Users/test/.config/opencode-guardian.toml",
            "/tmp/opencode-guardian.log",
        );

        assert!(rendered.contains("<string>/Applications/opencode-guardian</string>"));
        assert!(rendered.contains("<string>/Users/test/.config/opencode-guardian.toml</string>"));
        assert!(rendered.contains("<string>/tmp/opencode-guardian.log</string>"));
        assert!(!rendered.contains("{{LOG_PATH}}"));
    }
}
