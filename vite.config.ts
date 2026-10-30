import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
export default defineConfig(async ({ mode }) => ({
  plugins: [
    react(),
    ...(mode === 'offline-test'
      ? [(await import('./tests/core-server.mjs')).offlineCorePlugin()]
      : []),
  ],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  envPrefix: ['VITE_'],
  build: { target: 'safari17', assetsInlineLimit: 0 },
  worker: { format: 'es' as const },
}));
