#!/usr/bin/env python3
import csv
import json
import os
import random
import re
import signal
import socket
import string
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Optional


@dataclass
class LabConfig:
    model: str = os.environ.get("LEAK_LAB_MODEL", "opencode/gpt-5-nano")
    main_turns: int = int(os.environ.get("LEAK_LAB_MAIN_TURNS", "12"))
    fanout_turns: int = int(os.environ.get("LEAK_LAB_FANOUT_TURNS", "24"))
    run_timeout_secs: int = int(os.environ.get("LEAK_LAB_RUN_TIMEOUT", "45"))
    leak_like_peak_delta_kb: int = int(os.environ.get("LEAK_LAB_LEAK_PEAK_KB", "50000"))
    leak_like_end_delta_kb: int = int(os.environ.get("LEAK_LAB_LEAK_END_KB", "30000"))


def free_port() -> int:
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.bind(("127.0.0.1", 0))
    port = sock.getsockname()[1]
    sock.close()
    return port


def now_tag() -> str:
    return time.strftime("%Y%m%d-%H%M%S", time.localtime())


def random_token(n: int = 12) -> str:
    return "".join(random.choice(string.ascii_lowercase) for _ in range(n))


def rss_kb(pid: int) -> int:
    cmd = ["/bin/ps", "-o", "rss=", "-p", str(pid)]
    result = subprocess.run(cmd, capture_output=True, text=True, check=False)
    text = result.stdout.strip()
    return int(text) if text.isdigit() else 0


def process_table() -> list[tuple[int, int, str]]:
    result = subprocess.run(
        ["/bin/ps", "-axo", "pid=,ppid=,command="],
        capture_output=True,
        text=True,
        check=False,
    )
    rows: list[tuple[int, int, str]] = []
    for line in result.stdout.splitlines():
        line = line.strip()
        if not line:
            continue
        parts = line.split(None, 2)
        if len(parts) < 3:
            continue
        try:
            pid = int(parts[0])
            ppid = int(parts[1])
        except ValueError:
            continue
        rows.append((pid, ppid, parts[2]))
    return rows


def descendants(
    root_pid: int, table: list[tuple[int, int, str]]
) -> list[tuple[int, str]]:
    children_by_parent: dict[int, list[int]] = {}
    command_by_pid: dict[int, str] = {}
    for pid, ppid, command in table:
        children_by_parent.setdefault(ppid, []).append(pid)
        command_by_pid[pid] = command

    queue = [root_pid]
    out: list[tuple[int, str]] = []
    seen: set[int] = set()
    while queue:
        current = queue.pop(0)
        for child in children_by_parent.get(current, []):
            if child in seen:
                continue
            seen.add(child)
            out.append((child, command_by_pid.get(child, "")))
            queue.append(child)
    return out


def discover_target_pid(launcher_pid: int) -> Optional[int]:
    table = process_table()
    desc = descendants(launcher_pid, table)
    if not desc:
        return None

    for pid, command in desc:
        if "opencode serve" in command:
            return pid

    for pid, command in desc:
        if "opencode" in command:
            return pid

    return max(desc, key=lambda item: rss_kb(item[0]))[0]


def walk_stats(root: Path) -> tuple[int, int]:
    if not root.exists():
        return 0, 0

    files = 0
    total = 0
    for dirpath, _, filenames in os.walk(root):
        for filename in filenames:
            path = Path(dirpath) / filename
            try:
                stat = path.stat()
            except FileNotFoundError:
                continue
            files += 1
            total += stat.st_size
    return files, total


def write_csv(path: Path, rows: list[dict[str, int | str]]) -> None:
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=["iteration", "rss_kb", "delta_kb"])
        writer.writeheader()
        writer.writerows(rows)


def parse_session_id(jsonl_path: Path) -> Optional[str]:
    text = jsonl_path.read_text(encoding="utf-8", errors="ignore")
    matches = re.findall(r'"sessionID":"(ses_[A-Za-z0-9]+)"', text)
    return matches[0] if matches else None


def parse_session_ids(text: str) -> list[str]:
    return sorted(set(re.findall(r"\bses_[A-Za-z0-9]+\b", text)))


def main() -> int:
    cfg = LabConfig()
    port = free_port()
    lab = Path(f"/private/tmp/opencode-leak-lab3-{now_tag()}")
    home = lab / "home"
    xdg_data = lab / "xdg-data"
    xdg_config = lab / "xdg-config"
    tmpdir = lab / "tmp"
    logs = lab / "logs"
    for directory in (home, xdg_data, xdg_config, tmpdir, logs):
        directory.mkdir(parents=True, exist_ok=True)

    profile = "(version 1) (allow default)"

    env_pairs = {
        "HOME": str(home),
        "XDG_DATA_HOME": str(xdg_data),
        "XDG_CONFIG_HOME": str(xdg_config),
        "TMPDIR": str(tmpdir),
        "OPENCODE_NO_TELEMETRY": "1",
    }

    def sb_env_cmd(opencode_args: list[str]) -> list[str]:
        cmd = ["/usr/bin/sandbox-exec", "-p", profile, "/usr/bin/env"]
        for key, value in env_pairs.items():
            cmd.append(f"{key}={value}")
        cmd.extend(opencode_args)
        return cmd

    server_log_path = logs / "server.log"
    server_proc = subprocess.Popen(
        sb_env_cmd(
            [
                "opencode",
                "serve",
                "--hostname",
                "127.0.0.1",
                "--port",
                str(port),
                "--print-logs",
                "--log-level",
                "WARN",
            ]
        ),
        stdout=server_log_path.open("w", encoding="utf-8"),
        stderr=subprocess.STDOUT,
        text=True,
    )

    time.sleep(4)
    if server_proc.poll() is not None:
        print(json.dumps({"error": "server_start_failed", "lab": str(lab)}, indent=2))
        return 1

    target_pid = discover_target_pid(server_proc.pid) or server_proc.pid

    rows: list[dict[str, int | str]] = []
    warmup = []
    for _ in range(8):
        warmup.append(rss_kb(target_pid))
        time.sleep(1)

    baseline = (
        min(value for value in warmup if value > 0)
        if any(value > 0 for value in warmup)
        else rss_kb(server_proc.pid)
    )
    rows.append({"iteration": "0", "rss_kb": baseline, "delta_kb": 0})

    errors = 0
    first_client_path = logs / "client-main-0.jsonl"
    with (
        first_client_path.open("w", encoding="utf-8") as out,
        (logs / "client-errors.log").open("a", encoding="utf-8") as err,
    ):
        try:
            result = subprocess.run(
                sb_env_cmd(
                    [
                        "opencode",
                        "run",
                        "Start memory lab. Reply exactly: ok",
                        "--attach",
                        f"http://127.0.0.1:{port}",
                        "-m",
                        cfg.model,
                        "--format",
                        "json",
                        "--thinking",
                        "false",
                        "--title",
                        "leak-lab3-main",
                    ]
                ),
                stdout=out,
                stderr=err,
                timeout=cfg.run_timeout_secs,
                check=False,
                text=True,
            )
            if result.returncode != 0:
                errors += 1
        except subprocess.TimeoutExpired:
            err.write("timeout initial main turn\n")
            errors += 1

    session_id = parse_session_id(first_client_path)
    rows.append(
        {
            "iteration": "main-0",
            "rss_kb": rss_kb(target_pid),
            "delta_kb": rss_kb(target_pid) - baseline,
        }
    )

    if session_id:
        for index in range(1, cfg.main_turns + 1):
            prompt = "Continue memory lab. Reply exactly: ok " + random_token(6)
            client_path = logs / f"client-main-{index}.jsonl"
            with (
                client_path.open("w", encoding="utf-8") as out,
                (logs / "client-errors.log").open("a", encoding="utf-8") as err,
            ):
                try:
                    result = subprocess.run(
                        sb_env_cmd(
                            [
                                "opencode",
                                "run",
                                prompt,
                                "--attach",
                                f"http://127.0.0.1:{port}",
                                "-m",
                                cfg.model,
                                "--format",
                                "json",
                                "--thinking",
                                "false",
                                "--session",
                                session_id,
                            ]
                        ),
                        stdout=out,
                        stderr=err,
                        timeout=cfg.run_timeout_secs,
                        check=False,
                        text=True,
                    )
                    if result.returncode != 0:
                        errors += 1
                except subprocess.TimeoutExpired:
                    err.write(f"timeout main turn {index}\n")
                    errors += 1

            current = rss_kb(target_pid)
            rows.append(
                {
                    "iteration": f"main-{index}",
                    "rss_kb": current,
                    "delta_kb": current - baseline,
                }
            )

    for index in range(1, cfg.fanout_turns + 1):
        prompt = f"Fanout session {index}. Reply exactly: ok {random_token(6)}"
        client_path = logs / f"client-fanout-{index}.jsonl"
        with (
            client_path.open("w", encoding="utf-8") as out,
            (logs / "client-errors.log").open("a", encoding="utf-8") as err,
        ):
            try:
                result = subprocess.run(
                    sb_env_cmd(
                        [
                            "opencode",
                            "run",
                            prompt,
                            "--attach",
                            f"http://127.0.0.1:{port}",
                            "-m",
                            cfg.model,
                            "--format",
                            "json",
                            "--thinking",
                            "false",
                            "--title",
                            f"leak-lab3-fanout-{index}",
                        ]
                    ),
                    stdout=out,
                    stderr=err,
                    timeout=cfg.run_timeout_secs,
                    check=False,
                    text=True,
                )
                if result.returncode != 0:
                    errors += 1
            except subprocess.TimeoutExpired:
                err.write(f"timeout fanout turn {index}\n")
                errors += 1

        current = rss_kb(target_pid)
        rows.append(
            {
                "iteration": f"fanout-{index}",
                "rss_kb": current,
                "delta_kb": current - baseline,
            }
        )

    storage_path = xdg_data / "opencode" / "storage"
    files_before, bytes_before = walk_stats(storage_path)

    peak = max(row["rss_kb"] for row in rows if isinstance(row["rss_kb"], int))
    end = rows[-1]["rss_kb"] if rows else baseline
    if not isinstance(end, int):
        end = baseline
    peak_delta = peak - baseline
    end_delta = end - baseline
    leak_like = (
        peak_delta >= cfg.leak_like_peak_delta_kb
        or end_delta >= cfg.leak_like_end_delta_kb
    )

    term_sent = False
    kill_sent = False
    if server_proc.poll() is None:
        server_proc.terminate()
        term_sent = True
        try:
            server_proc.wait(timeout=6)
        except subprocess.TimeoutExpired:
            server_proc.kill()
            kill_sent = True
            server_proc.wait(timeout=5)

    session_before_txt = logs / "session-list-before.txt"
    with session_before_txt.open("w", encoding="utf-8") as out:
        subprocess.run(
            sb_env_cmd(["opencode", "session", "list"]),
            stdout=out,
            stderr=subprocess.STDOUT,
            check=False,
            text=True,
        )
    ids_before = parse_session_ids(
        session_before_txt.read_text(encoding="utf-8", errors="ignore")
    )

    session_delete_log = logs / "session-delete.log"
    with session_delete_log.open("w", encoding="utf-8") as out:
        for session in ids_before:
            subprocess.run(
                sb_env_cmd(["opencode", "session", "delete", session]),
                stdout=out,
                stderr=subprocess.STDOUT,
                check=False,
                text=True,
            )

    session_after_txt = logs / "session-list-after.txt"
    with session_after_txt.open("w", encoding="utf-8") as out:
        subprocess.run(
            sb_env_cmd(["opencode", "session", "list"]),
            stdout=out,
            stderr=subprocess.STDOUT,
            check=False,
            text=True,
        )
    ids_after = parse_session_ids(
        session_after_txt.read_text(encoding="utf-8", errors="ignore")
    )

    files_after, bytes_after = walk_stats(storage_path)
    write_csv(logs / "rss.csv", rows)

    summary = {
        "lab": str(lab),
        "server_pid": target_pid,
        "launcher_pid": server_proc.pid,
        "model": cfg.model,
        "baseline_rss_kb": baseline,
        "peak_rss_kb": peak,
        "end_rss_kb": end,
        "peak_delta_kb": peak_delta,
        "end_delta_kb": end_delta,
        "sample_count": len(rows),
        "leak_like_threshold_hit": leak_like,
        "term_sent": term_sent,
        "kill_sent": kill_sent,
        "run_errors": errors,
        "main_session_id": session_id,
        "sessions_before_cleanup": len(ids_before),
        "sessions_after_cleanup": len(ids_after),
        "sessions_deleted_attempted": len(ids_before),
        "storage_files_before": files_before,
        "storage_files_after": files_after,
        "storage_bytes_before": bytes_before,
        "storage_bytes_after": bytes_after,
        "logs": {
            "server": str(server_log_path),
            "rss": str(logs / "rss.csv"),
            "session_before": str(session_before_txt),
            "session_after": str(session_after_txt),
            "session_delete": str(session_delete_log),
            "client_errors": str(logs / "client-errors.log"),
        },
    }

    summary_path = logs / "summary.json"
    summary_path.write_text(json.dumps(summary, indent=2), encoding="utf-8")

    print(json.dumps(summary, indent=2))
    print(f"summary_path={summary_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
