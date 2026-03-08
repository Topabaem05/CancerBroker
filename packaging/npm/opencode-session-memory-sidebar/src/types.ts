export type ReasonCode =
  | "sidebar_unsupported"
  | "platform_unsupported"
  | "session_api_unavailable"
  | "unavailable_shared_process"
  | "unavailable_no_pid"
  | "permission_denied"
  | "stale";

export type CapabilityDisabledReasonCode = Extract<
  ReasonCode,
  "sidebar_unsupported" | "platform_unsupported" | "session_api_unavailable"
>;

export type RamUnavailableReasonCode = Extract<
  ReasonCode,
  "unavailable_shared_process" | "unavailable_no_pid" | "permission_denied" | "stale"
>;

export type RamMetric = "rss_bytes" | "phys_footprint_bytes";

export interface SessionTokenUsage {
  readonly inputTokens: number;
  readonly outputTokens: number;
  readonly reasoningTokens: number;
  readonly cacheReadTokens: number;
  readonly totalTokens: number;
}

export interface SessionRamStateExact {
  readonly mappingState: "exact";
  readonly metric: RamMetric;
  readonly bytes: number;
  readonly sampledAtIso: string;
}

export interface SessionRamStateUnavailable {
  readonly mappingState: "unavailable";
  readonly reason: RamUnavailableReasonCode;
  readonly sampledAtIso: string;
  readonly bytes?: never;
  readonly metric?: never;
}

export type SessionRamState = SessionRamStateExact | SessionRamStateUnavailable;

export interface SessionMemoryRow {
  readonly sessionId: string;
  readonly isCurrent: boolean;
  readonly tokenUsage: SessionTokenUsage;
  readonly ram: SessionRamState;
}

export interface SessionMemorySummary {
  readonly liveSessionCount: number;
  readonly exactRamCoverageCount: number;
  readonly unavailableRamCount: number;
  readonly exactRamTotalBytes: number;
}

export interface CapabilityEnabledState {
  readonly state: "enabled";
  readonly platform: "darwin";
  readonly v1RamMetric: "rss_bytes";
}

export interface CapabilityDisabledState {
  readonly state: "disabled";
  readonly reason: CapabilityDisabledReasonCode;
  readonly message: string;
}

export type RuntimeCapabilityState = CapabilityEnabledState | CapabilityDisabledState;

export function createRamState(
  input:
    | {
        readonly mappingState: "exact";
        readonly bytes: number;
        readonly sampledAtIso: string;
        readonly metric?: RamMetric;
      }
    | {
        readonly mappingState: "unavailable";
        readonly reason: RamUnavailableReasonCode;
        readonly sampledAtIso: string;
        readonly bytes?: never;
      },
): SessionRamState {
  if (input.mappingState === "exact") {
    if (!Number.isFinite(input.bytes) || input.bytes < 0) {
      throw new Error("exact RAM state requires a non-negative finite byte value");
    }
    return {
      mappingState: "exact",
      bytes: input.bytes,
      metric: input.metric ?? "rss_bytes",
      sampledAtIso: input.sampledAtIso,
    };
  }

  if ("bytes" in input && typeof input.bytes === "number") {
    throw new Error("numeric RAM is only valid when mappingState is exact");
  }

  return {
    mappingState: "unavailable",
    reason: input.reason,
    sampledAtIso: input.sampledAtIso,
  };
}
