export function preferredMime(): string | null {
  if (typeof MediaRecorder === 'undefined') return null;
  return (
    ['audio/mp4', 'audio/webm;codecs=opus', 'audio/webm'].find((t) =>
      MediaRecorder.isTypeSupported(t),
    ) ?? null
  );
}
export function encodeWav(samples: Float32Array, sampleRate = 16000): Uint8Array {
  const buffer = new ArrayBuffer(44 + samples.length * 2),
    view = new DataView(buffer);
  const text = (at: number, s: string) => {
    for (let i = 0; i < s.length; i++) view.setUint8(at + i, s.charCodeAt(i));
  };
  text(0, 'RIFF');
  view.setUint32(4, buffer.byteLength - 8, true);
  text(8, 'WAVE');
  text(12, 'fmt ');
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 1, true);
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, sampleRate * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  text(36, 'data');
  view.setUint32(40, samples.length * 2, true);
  samples.forEach((v, i) => {
    const s = Math.max(-1, Math.min(1, v));
    view.setInt16(44 + i * 2, s < 0 ? s * 32768 : s * 32767, true);
  });
  return new Uint8Array(buffer);
}
export async function recordingToWav(blob: Blob): Promise<Uint8Array> {
  if (blob.size > 25_000_000)
    throw new Error('Recording is too large. Please record a shorter thought.');
  const context = new AudioContext();
  try {
    const decoded = await context.decodeAudioData(await blob.arrayBuffer());
    if (decoded.duration < 0.2 || decoded.duration > 120)
      throw new Error('Record between 0.2 and 120 seconds.');
    const offline = new OfflineAudioContext(1, Math.ceil(decoded.duration * 16000), 16000);
    const source = offline.createBufferSource();
    source.buffer = decoded;
    source.connect(offline.destination);
    source.start();
    const rendered = await offline.startRendering();
    return encodeWav(rendered.getChannelData(0));
  } finally {
    await context.close();
  }
}
