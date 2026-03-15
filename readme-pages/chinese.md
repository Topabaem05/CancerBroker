# 中文

- [返回首页](../README.md)
- [语言索引](index.md)

语言: [English](english.md) | [中文](chinese.md) | [Español](spanish.md) | [한국어](korean.md) | [日本語](japanese.md)

CancerBroker 是一个面向 Opencode 进程的 Rust 清理工具。它会跟踪 PID、PGID、监听端口和详细的打开资源，检测重复的 RSS 增长，并在发送信号前通过安全检查来清理任务范围内的进程。

## 安装

```bash
cargo install --git https://github.com/Topabaem05/CancerBroker.git
```

## Opencode 设置

```bash
cancerbroker setup
```

该命令现在会在 TTY 中打开一个最小化的逐行设置向导，然后：

- 使用 `cancerbroker mcp` 将 CancerBroker 注册为本地 Opencode MCP 服务器
- 将 rust-analyzer 内存保护设置写入 `~/.config/cancerbroker/config.toml`

如果你想直接使用当前机器推荐的默认值而不进行交互，可使用非交互模式：

```bash
cancerbroker setup --non-interactive
```

### 交互式设置示例

示例命令：

```bash
cancerbroker setup
```

示例提示流程：

```text
CancerBroker setup will:
- register the local MCP server in OpenCode
- configure the rust-analyzer memory guard for this machine
Detected system RAM: 36 GB. Press Enter to accept the default shown in brackets.

Enable rust-analyzer memory protection? [Y/n]
  When enabled, CancerBroker watches rust-analyzer memory and can clean it up after repeated over-limit samples.
>

Memory cap in GB [6]
  CancerBroker starts counting rust-analyzer as over the limit after it stays above this amount of RAM.
>

Consecutive over-limit samples before action [3]
  This avoids reacting to a single short memory spike.
>

Startup grace in seconds [300]
  rust-analyzer often spikes during initial indexing, so counting starts after this delay.
>

Cooldown after remediation in seconds [1800]
  This prevents repeated remediation loops after rust-analyzer restarts.
>
```

说明：

- 在任意提示处按 `Enter` 会接受默认值并继续。
- 内存输入使用整数 `GB`，但在 guardian 配置中会以字节形式保存。
- 如果再次运行 setup，现有 guardian 设置会作为新的默认值复用。
- 设置向导不会修改全局 `mode`；如果 guardian 配置仍然是 `observe`，rust-analyzer 内存保护只会记录候选项，不会终止进程。

## 在 Opencode 中如何工作

```mermaid
flowchart TD
    Setup[`cancerbroker setup`]
    OpenCode[Opencode]
    Config[`~/.config/opencode/opencode.json`]
    MCP[`cancerbroker mcp`]
    Tools["status, scan, scan_resources, scan_leaks, cleanup, list_evidence"]
    Guard["GuardianConfig\n--config, CANCERBROKER_CONFIG, default config"]
    Inventory[`进程与工件清单\nsrc/monitor/process.rs\nsrc/monitor/storage.rs`]
    Runtime[`策略运行时\nsrc/runtime.rs\nsrc/policy.rs\nsrc/evidence.rs`]
    Daemon[`完成事件守护进程\nsrc/ipc.rs\nsrc/daemon.rs\nsrc/autocleanup.rs`]
    Resolve[`会话解析与修复\nsrc/resolution.rs\nsrc/remediation.rs\nsrc/safety.rs`]

    Setup --> Config
    Config -->|registers local MCP command| MCP
    OpenCode -->|launches local MCP over stdio| MCP
    MCP --> Tools
    Tools --> Guard
    Tools --> Inventory
    Tools --> Runtime
    Runtime --> Resolve
    OpenCode -->|completion events| Daemon
    Daemon --> Inventory
    Daemon --> Resolve
```

- `cancerbroker setup` 会更新 `~/.config/opencode/opencode.json`，使 Opencode 可以把 `cancerbroker mcp` 作为本地 MCP 服务器启动。
- `cancerbroker mcp` 从 `src/mcp.rs` 提供 MCP 工具；`status`、`scan`、`scan_resources`、`scan_leaks`、`cleanup` 和 `list_evidence` 是面向 Opencode 的入口。
- `cleanup` 和 `run-once` 共享同一条策略执行路径：`src/cli.rs` -> `src/runtime.rs` -> `src/policy.rs` -> `src/evidence.rs`。
- `daemon` 是长期运行的清理路径：`src/cli.rs` -> `src/daemon.rs` -> `src/ipc.rs` -> `src/autocleanup.rs` -> `src/resolution.rs` / `src/remediation.rs`。
- 通过 `src/config.rs` 和 `src/safety.rs` 中的 `required_command_markers` 与同 UID 安全检查，进程和工件清理会被限制在 Opencode/OpenAgent 工作负载范围内。

## 快速开始

```bash
cancerbroker --config fixtures/config/observe-only.toml status --json
cancerbroker --config fixtures/config/observe-only.toml run-once --json
cancerbroker --config fixtures/config/completion-cleanup.toml daemon --json --max-events 128
```

## 功能说明

- 跟踪实时进程身份信息，包括 PID、父 PID、PGID、UID、内存、CPU 和监听端口。
- 通过命令标记安全规则解析与 Opencode 相关的进程和会话工件。
- 在清理前捕获详细的打开文件和套接字端点信息。
- 检测实时 RSS 泄漏候选项，并在 daemon 模式下执行清理。
- 先用 `SIGTERM` 终止目标；如果目标在超时后仍未退出，则升级到 `SIGKILL`。

## 验证

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo build --workspace
```

## 沙盒终止验证

用于验证 leak-enforcement PID 终止路径的专用测试：

```bash
cargo test --workspace run_leak_enforcement_with_inventory_terminates_leaking_process_in_enforce_mode -- --nocapture
```

沙盒验证中的预期信号结果：

```json
{"returncode": -15, "signal": 15}
{"returncode": -9, "signal": 9}
```

- `signal: 15` 表示目标在 `SIGTERM` 后退出。
- `signal: 9` 表示目标忽略了 `SIGTERM`，CancerBroker 随后升级为 `SIGKILL`。
