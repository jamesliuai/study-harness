import { invoke } from '@tauri-apps/api/core';
export async function sourceCall<T>(method: string, request: unknown): Promise<T> {
  const body = new TextEncoder().encode(JSON.stringify({ method, request }));
  if (import.meta.env.MODE === 'offline-test') {
    const response = await fetch('/__core', {
      method: 'POST',
      body: JSON.stringify({ command: 'source_rpc', args: { body: Array.from(body) } }),
    });
    const reply = await response.json();
    if (!reply.ok) throw reply.error;
    return reply.result as T;
  }
  return invoke<T>('source_rpc', body);
}
export function bytesToBase64(bytes: Uint8Array) {
  let binary = '';
  for (let i = 0; i < bytes.length; i += 8192)
    binary += String.fromCharCode(...bytes.subarray(i, i + 8192));
  return btoa(binary);
}
export function base64ToBytes(value: string) {
  return Uint8Array.from(atob(value), (c) => c.charCodeAt(0));
}
