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
Description=cancerbroker sidecar
After=network.target

[Service]
Type=simple
WorkingDirectory={{WORKDIR}}
ExecStart={{EXEC_PATH}} --config {{CONFIG_PATH}} run-once --json
Restart=on-failure

[Install]
WantedBy=multi-user.target
"#;

pub fn render_windows_service_install(exec_path: &str, config_path: &str) -> String {
    render_template(
        WINDOWS_SERVICE_TEMPLATE,
        &[
            ("{{EXEC_PATH}}", exec_path),
            ("{{CONFIG_PATH}}", config_path),
        ],
    )
}

const WINDOWS_SERVICE_TEMPLATE: &str = r#"sc.exe create cancerbroker ^
    binPath= "{{EXEC_PATH}} --config {{CONFIG_PATH}} run-once --json" ^
    start= auto ^
    DisplayName= "cancerbroker sidecar"
sc.exe description cancerbroker "Automated cleanup sidecar for opencode sessions"
sc.exe start cancerbroker
"#;

const LAUNCHD_TEMPLATE: &str = r#"<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">
<plist version=\"1.0\">
  <dict>
    <key>Label</key>
    <string>com.cancerbroker</string>
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
    use super::{
        render_launchd_plist, render_systemd_unit, render_template, render_windows_service_install,
    };

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
            "/usr/local/bin/cancerbroker",
            "/etc/cancerbroker.toml",
            "/var/lib/cancerbroker",
        );

        assert!(rendered.contains("WorkingDirectory=/var/lib/cancerbroker"));
        assert!(rendered.contains(
            "ExecStart=/usr/local/bin/cancerbroker --config /etc/cancerbroker.toml run-once --json"
        ));
        assert!(!rendered.contains("{{EXEC_PATH}}"));
    }

    #[test]
    fn render_launchd_plist_injects_program_arguments_and_log_path() {
        let rendered = render_launchd_plist(
            "/Applications/cancerbroker",
            "/Users/test/.config/cancerbroker.toml",
            "/tmp/cancerbroker.log",
        );

        assert!(rendered.contains("<string>/Applications/cancerbroker</string>"));
        assert!(rendered.contains("<string>/Users/test/.config/cancerbroker.toml</string>"));
        assert!(rendered.contains("<string>/tmp/cancerbroker.log</string>"));
        assert!(!rendered.contains("{{LOG_PATH}}"));
    }

    #[test]
    fn render_windows_service_install_injects_exec_and_config_paths() {
        let rendered = render_windows_service_install(
            r"C:\Program Files\cancerbroker\cancerbroker.exe",
            r"C:\ProgramData\cancerbroker\cancerbroker.toml",
        );

        assert!(rendered.contains(r"C:\Program Files\cancerbroker\cancerbroker.exe"));
        assert!(rendered.contains(r"C:\ProgramData\cancerbroker\cancerbroker.toml"));
        assert!(rendered.contains("sc.exe create cancerbroker"));
        assert!(!rendered.contains("{{EXEC_PATH}}"));
        assert!(!rendered.contains("{{CONFIG_PATH}}"));
    }
}
