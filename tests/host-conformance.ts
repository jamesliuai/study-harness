// Runs only as an explicit test entry in a browser. Kernels stay in the real module worker path.
import { createInstalledCatalog } from '@explorable-models/catalog';
import { runHostPortConformance } from '@explorable-models/activity/conformance';
import { nativeHost } from '../src/visualizer/host';
import manifest from '../vendor/visualizer/manifest.json';
const call = async (command: string, args: unknown = {}) => {
  const r = await fetch('/__core', { method: 'POST', body: JSON.stringify({ command, args }) });
  const reply = await r.json();
  if (!reply.ok) throw reply.error;
  return reply.result;
};
declare global {
  interface Window {
    hostConformance: ReturnType<typeof runHostPortConformance>;
  }
}
const host = () => nativeHost();
window.hostConformance = runHostPortConformance(host(), {
  subject: 'actual-host',
  host: await call('test.identity'),
  packages: manifest.packages.map(({ name, version, sha256 }) => ({ name, version, sha256 })),
  catalog: createInstalledCatalog(),
  hooks: {
    openActivity: (q) => call('test.openActivity', q),
    restart: async () => {
      await call('test.restart');
      return host();
    },
    respond: (q) => call('test.respond', q),
    hostSessionRevision: (q) => call('test.hostSessionRevision', q),
    authorizeReveal: (q) => call('test.authorizeReveal', q),
    startIndependentTask: (q) => call('test.startIndependentTask', q),
    armCrash: (point) => call('test.armCrash', { point, terminate: true }),
  },
});
window.hostConformance.then((run) => {
  document.querySelector('main')!.textContent = JSON.stringify(run);
});
