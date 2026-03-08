import {
  resolveProcessSampleTimeoutMs,
  sampleMacOsProcess,
  type MacOsProcessSample,
  type SampleMacOsProcessInput,
} from "./process-macos";
import { createRamState, type SessionRamState } from "./types";

export interface SessionProcessMappingCandidate {
  readonly sessionId: string;
  readonly pid?: number;
  readonly startTimeIso?: string;
}

export interface PreviousExactRamSample {
  readonly pid: number;
  readonly startTimeIso: string;
  readonly sampledAtIso: string;
}

export interface SessionRamAttribution {
  readonly sessionId: string;
  readonly ram: SessionRamState;
}

export interface AttributeSessionRamInput {
  readonly sessions: readonly SessionProcessMappingCandidate[];
  readonly sampleTimeoutMs?: number;
  readonly now?: () => Date;
  readonly sampleProcess?: (input: SampleMacOsProcessInput) => Promise<MacOsProcessSample>;
  readonly previousExactSamplesBySessionId?: ReadonlyMap<string, PreviousExactRamSample>;
}

export async function attributeSessionRam(
  input: AttributeSessionRamInput,
): Promise<SessionRamAttribution[]> {
  const now = input.now ?? (() => new Date());
  const fallbackSampledAtIso = now().toISOString();
  const sampleTimeoutMs = resolveProcessSampleTimeoutMs(input.sampleTimeoutMs);
  const sampleProcess = input.sampleProcess ?? sampleMacOsProcess;
  const sharedPids = collectSharedPids(input.sessions);
  const sampleByPid = new Map<number, Promise<MacOsProcessSample>>();
  const rows: SessionRamAttribution[] = [];

  for (const session of input.sessions) {
    const pid = session.pid;
    if (!isUsablePid(pid)) {
      rows.push({
        sessionId: session.sessionId,
        ram: createRamState({
          mappingState: "unavailable",
          reason: "unavailable_no_pid",
          sampledAtIso: fallbackSampledAtIso,
        }),
      });
      continue;
    }

    const sessionWithPid = {
      ...session,
      pid,
    };

    if (sharedPids.has(pid)) {
      rows.push({
        sessionId: session.sessionId,
        ram: createRamState({
          mappingState: "unavailable",
          reason: "unavailable_shared_process",
          sampledAtIso: fallbackSampledAtIso,
        }),
      });
      continue;
    }

    const expectedStartTimeIso = normalizeStartTimeIso(session.startTimeIso);
    if (!expectedStartTimeIso) {
      rows.push({
        sessionId: session.sessionId,
        ram: createRamState({
          mappingState: "unavailable",
          reason: "stale",
          sampledAtIso: pickStaleTimestamp({
            session: sessionWithPid,
            fallbackSampledAtIso,
            previousExactSamplesBySessionId: input.previousExactSamplesBySessionId,
          }),
        }),
      });
      continue;
    }

    const cachedSamplePromise = sampleByPid.get(pid);
    const samplePromise =
      cachedSamplePromise ??
      sampleProcess({
        pid,
        timeoutMs: sampleTimeoutMs,
        now,
      });

    if (!cachedSamplePromise) {
      sampleByPid.set(pid, samplePromise);
    }

    const sample = await samplePromise;
    rows.push({
      sessionId: session.sessionId,
      ram: classifySampleForSession({
        session: sessionWithPid,
        sample,
        expectedStartTimeIso,
        previousExactSamplesBySessionId: input.previousExactSamplesBySessionId,
      }),
    });
  }

  return rows;
}

function classifySampleForSession(input: {
  readonly session: SessionProcessMappingCandidate & { readonly pid: number };
  readonly sample: MacOsProcessSample;
  readonly expectedStartTimeIso: string;
  readonly previousExactSamplesBySessionId?: ReadonlyMap<string, PreviousExactRamSample>;
}): SessionRamState {
  if (input.sample.state === "sampled") {
    const observedStartTimeIso = normalizeStartTimeIso(input.sample.identity.startTimeIso);
    const isSameInstance =
      input.sample.identity.pid === input.session.pid &&
      observedStartTimeIso === input.expectedStartTimeIso;

    if (isSameInstance) {
      return createRamState({
        mappingState: "exact",
        metric: input.sample.metric,
        bytes: input.sample.bytes,
        sampledAtIso: input.sample.sampledAtIso,
      });
    }

    return createRamState({
      mappingState: "unavailable",
      reason: "stale",
      sampledAtIso: pickStaleTimestamp({
        session: input.session,
        expectedStartTimeIso: input.expectedStartTimeIso,
        fallbackSampledAtIso: input.sample.sampledAtIso,
        previousExactSamplesBySessionId: input.previousExactSamplesBySessionId,
      }),
    });
  }

  if (input.sample.state === "permission_denied") {
    return createRamState({
      mappingState: "unavailable",
      reason: "permission_denied",
      sampledAtIso: input.sample.sampledAtIso,
    });
  }

  return createRamState({
    mappingState: "unavailable",
    reason: "stale",
    sampledAtIso: pickStaleTimestamp({
      session: input.session,
      expectedStartTimeIso: input.expectedStartTimeIso,
      fallbackSampledAtIso: input.sample.sampledAtIso,
      previousExactSamplesBySessionId: input.previousExactSamplesBySessionId,
    }),
  });
}

function isUsablePid(pid: number | undefined): pid is number {
  return typeof pid === "number" && Number.isInteger(pid) && pid > 0;
}

function normalizeStartTimeIso(startTimeIso: string | undefined): string | null {
  if (!startTimeIso) {
    return null;
  }

  const parsed = Date.parse(startTimeIso);
  if (!Number.isFinite(parsed)) {
    return null;
  }

  return new Date(parsed).toISOString();
}

function collectSharedPids(sessions: readonly SessionProcessMappingCandidate[]): Set<number> {
  const countsByPid = new Map<number, number>();

  for (const session of sessions) {
    if (!isUsablePid(session.pid)) {
      continue;
    }

    countsByPid.set(session.pid, (countsByPid.get(session.pid) ?? 0) + 1);
  }

  return new Set(
    Array.from(countsByPid.entries())
      .filter(([, count]) => count > 1)
      .map(([pid]) => pid),
  );
}

function pickStaleTimestamp(input: {
  readonly session: SessionProcessMappingCandidate & { readonly pid: number };
  readonly fallbackSampledAtIso: string;
  readonly expectedStartTimeIso?: string;
  readonly previousExactSamplesBySessionId?: ReadonlyMap<string, PreviousExactRamSample>;
}): string {
  const previous = input.previousExactSamplesBySessionId?.get(input.session.sessionId);
  if (!previous || previous.pid !== input.session.pid) {
    return input.fallbackSampledAtIso;
  }

  if (input.expectedStartTimeIso) {
    const previousStartTimeIso = normalizeStartTimeIso(previous.startTimeIso);
    if (previousStartTimeIso !== input.expectedStartTimeIso) {
      return input.fallbackSampledAtIso;
    }
  }

  return previous.sampledAtIso;
}
