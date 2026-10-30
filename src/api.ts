import { invoke, isTauri } from '@tauri-apps/api/core';
export const desktop = isTauri();
export async function command<T>(name: string, args?: Record<string, unknown>): Promise<T> {
  if (import.meta.env.MODE === 'offline-test') {
    const response = await fetch('/__core', {
      method: 'POST',
      body: JSON.stringify({ command: name, args }),
    });
    const reply = await response.json();
    if (!reply.ok) throw reply.error;
    return reply.result as T;
  }
  if (!desktop)
    throw new Error(
      'Open the packaged Study Harness app to use local storage, recording, and live providers.',
    );
  return invoke<T>(name, args);
}
export function money(micro: number) {
  return (micro / 1_000_000).toLocaleString('en-US', {
    style: 'currency',
    currency: 'USD',
    minimumFractionDigits: 2,
    maximumFractionDigits: 4,
  });
}
