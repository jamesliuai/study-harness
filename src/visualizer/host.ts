import { invoke } from '@tauri-apps/api/core';
import type { HostPort } from '@explorable-models/activity';

export async function visualCall<T>(method: string, request: unknown): Promise<T> {
  const body = new TextEncoder().encode(JSON.stringify({ method, request }));
  if (import.meta.env.MODE === 'offline-test') {
    const response = await fetch('/__core', {
      method: 'POST',
      body: JSON.stringify({ command: 'visualizer_rpc', args: { body: Array.from(body) } }),
    });
    const reply = await response.json();
    if (!reply.ok) throw reply.error;
    return reply.result as T;
  }
  return invoke<T>('visualizer_rpc', body);
}

export function nativeHost(onGrant?: () => void): HostPort {
  const grant = async <T>(method: string, request: unknown) => {
    const reply = await visualCall<T>(method, request);
    onGrant?.();
    return reply;
  };
  return {
    getPolicy: (q) => visualCall('getPolicy', q),
    registerActivity: (q) => visualCall('registerActivity', q),
    requestStageTransition: (q) => grant('requestStageTransition', q),
    recordExposureIntent: (q) => grant('recordExposureIntent', q),
    recordDisplayAck: (q) => visualCall('recordDisplayAck', q),
    recordAssistance: (q) => grant('recordAssistance', q),
    appendEvents: (q) => visualCall('appendEvents', q),
    getJournalCursor: (q) => visualCall('getJournalCursor', q),
    getActivityRecords: (q) => visualCall('getActivityRecords', q),
    saveArtifact: (q) => visualCall('saveArtifact', q),
  };
}
