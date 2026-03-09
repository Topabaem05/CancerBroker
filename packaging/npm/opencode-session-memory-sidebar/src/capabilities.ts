import type { CapabilityDisabledReasonCode, RuntimeCapabilityState } from "./types";

export interface RuntimeCapabilityProbeInput {
  readonly platform?: string;
  readonly sessionApi?: unknown;
}

export const DISABLED_REASON_MESSAGES: Record<CapabilityDisabledReasonCode, string> = {
  platform_unsupported: "Session Memory supports macOS only in v1 (platform_unsupported).",
  session_api_unavailable:
    "Required OpenCode session APIs are unreachable in this runtime (session_api_unavailable).",
};

export function probeRuntimeCapabilities(input: RuntimeCapabilityProbeInput = {}): RuntimeCapabilityState {
  const platform = input.platform ?? process.platform;

  if (platform !== "darwin") {
    return {
      state: "disabled",
      reason: "platform_unsupported",
      message: DISABLED_REASON_MESSAGES.platform_unsupported,
    };
  }

  if (!hasUsableSessionApi(input.sessionApi)) {
    return {
      state: "disabled",
      reason: "session_api_unavailable",
      message: DISABLED_REASON_MESSAGES.session_api_unavailable,
    };
  }

  return {
    state: "enabled",
    platform: "darwin",
    v1RamMetric: "rss_bytes",
  };
}

export function hasUsableSessionApi(sessionApi: unknown): boolean {
  if (!sessionApi || typeof sessionApi !== "object") {
    return false;
  }

  const candidate = sessionApi as Record<string, unknown>;
  return (
    typeof candidate.list === "function" &&
    typeof candidate.get === "function" &&
    typeof candidate.messages === "function"
  );
}
