// Test transport only: the unchanged production assets invoke the actual isolated Rust/SQLite store.
// No learning state, policy, grading or provider response is implemented here. Never bundled in the app.
globalThis.isTauri = true;
globalThis.__TAURI_INTERNALS__ = {
  invoke: async (command, args = {}) => {
    const raw = ArrayBuffer.isView(args)
      ? Array.from(new Uint8Array(args.buffer, args.byteOffset, args.byteLength))
      : args instanceof ArrayBuffer
        ? Array.from(new Uint8Array(args))
        : null;
    const response = await fetch('/__core', {
      method: 'POST',
      body: JSON.stringify({ command, args: raw ? { body: raw } : args }),
    });
    const reply = await response.json();
    if (!reply.ok) throw reply.error;
    return reply.result;
  },
};
