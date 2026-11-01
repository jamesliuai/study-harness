import { spawn } from 'node:child_process';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { createInterface } from 'node:readline';
import { mkdtemp } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';

export class OfflineCore {
  constructor(database) {
    this.database = database;
    this.pending = null;
    this.queue = Promise.resolve();
    this.child = null;
  }
  start() {
    const child = spawn(path.resolve('src-tauri/target/debug/offline-harness'), [this.database], {
      stdio: ['pipe', 'pipe', 'inherit'],
    });
    this.child = child;
    createInterface({ input: child.stdout }).on('line', (line) => {
      const pending = this.pending;
      this.pending = null;
      if (pending) {
        try {
          pending.resolve(JSON.parse(line));
        } catch (e) {
          pending.reject(e);
        }
      }
    });
    child.on('error', (e) => {
      this.pending?.reject(e);
      this.pending = null;
    });
    child.on('exit', (code, signal) => {
      if (this.child === child) this.child = null;
      this.pending?.resolve({
        ok: false,
        error: {
          code: 'PERSISTENCE_FAILED',
          message: 'The isolated core process ended.',
          reason: signal ?? String(code),
        },
      });
      this.pending = null;
    });
  }
  async restart() {
    const child = this.child;
    if (child) {
      const ended = new Promise((resolve) => child.once('exit', resolve));
      child.kill('SIGKILL');
      await ended;
    }
    this.start();
    return { ok: true, result: { processTermination: true } };
  }
  request(command, args = {}) {
    const job = async () => {
      if (command === 'test.identity')
        return {
          ok: true,
          result: {
            name: 'StudyHarness.RustSQLite',
            commit: execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(),
            dirty:
              execFileSync('git', ['status', '--porcelain'], { encoding: 'utf8' }).trim().length >
              0,
            binarySha256: createHash('sha256')
              .update(await readFile('src-tauri/target/debug/offline-harness'))
              .digest('hex'),
            signed: false,
            platform: {
              os:
                process.platform === 'darwin'
                  ? 'macOS'
                  : process.platform === 'win32'
                    ? 'Windows'
                    : 'Linux',
              osVersion:
                process.platform === 'darwin'
                  ? `${execFileSync('sw_vers', ['-productVersion'], { encoding: 'utf8' }).trim()} (Darwin ${os.release()})`
                  : os.release(),
              arch: os.arch(),
              webview: 'Playwright Chromium; real separate Rust process, protocol checks only',
            },
          },
        };
      if (command === 'test.restart') return this.restart();
      if (!this.child)
        return {
          ok: false,
          error: { code: 'PERSISTENCE_FAILED', message: 'Restart the isolated core.' },
        };
      return new Promise((resolve, reject) => {
        this.pending = { resolve, reject };
        this.child.stdin.write(JSON.stringify({ command, args }) + '\n');
      });
    };
    const result = this.queue.catch(() => {}).then(job);
    this.queue = result;
    return result;
  }
  close() {
    this.child?.kill('SIGTERM');
  }
}

export function offlineCorePlugin() {
  let core;
  return {
    name: 'offline-test-real-core',
    async configureServer(server) {
      const dir = await mkdtemp(path.join(os.tmpdir(), 'study-harness-ui-'));
      core = new OfflineCore(path.join(dir, 'offline.sqlite3'));
      core.start();
      server.httpServer?.once('close', () => core.close());
      // Exercise exact built assets under the native CSP. Only the transport script is injected;
      // the app still calls the real Rust authority. These routes exist only in offline-test mode.
      server.middlewares.use(async (req, res, next) => {
        const url = req.url?.split('?')[0] ?? '';
        const mime = {
          '.js': 'text/javascript',
          '.mjs': 'text/javascript',
          '.css': 'text/css',
          '.woff2': 'font/woff2',
          '.woff': 'font/woff',
          '.ttf': 'font/ttf',
          '.html': 'text/html',
        };
        let file;
        if (url === '/__production/' || url === '/__production/index.html')
          file = 'dist/index.html';
        else if (url === '/__production/transport.js') file = 'tests/production-transport.js';
        else if (
          /^\/(assets\/[^/]+|pdf\/(?:standard_fonts|cmaps)\/[^/]+)$/.test(url) &&
          !url.includes('..')
        )
          file = `dist${url}`;
        else return next();
        try {
          let bytes = await readFile(file);
          if (file === 'dist/index.html')
            bytes = Buffer.from(
              bytes
                .toString()
                .replace('<head>', '<head><script src="/__production/transport.js"></script>'),
            );
          const config = JSON.parse(await readFile('src-tauri/tauri.conf.json', 'utf8'));
          res.setHeader(
            'Content-Security-Policy',
            file.endsWith('.js') || file.endsWith('.mjs')
              ? "default-src 'none'; script-src 'self'; connect-src 'self'"
              : config.app.security.csp,
          );
          res.setHeader('Content-Type', mime[path.extname(file)] ?? 'application/octet-stream');
          res.end(bytes);
        } catch {
          res.statusCode = 404;
          res.end('Build the production assets first.');
        }
      });
      server.middlewares.use('/__core', async (req, res) => {
        if (req.method !== 'POST') {
          res.statusCode = 405;
          res.end();
          return;
        }
        const origin = req.headers.origin;
        if (origin && !/^http:\/\/(?:127\.0\.0\.1|localhost):1420$/.test(origin)) {
          res.statusCode = 403;
          res.end();
          return;
        }
        let body = '';
        for await (const chunk of req) {
          body += chunk;
          if (body.length > 17 * 1024 * 1024) {
            res.statusCode = 413;
            res.end();
            return;
          }
        }
        try {
          const q = JSON.parse(body);
          const result = await core.request(q.command, q.args);
          res.setHeader('Content-Type', 'application/json');
          res.end(JSON.stringify(result));
        } catch {
          res.statusCode = 500;
          res.end(JSON.stringify({ ok: false, error: 'Offline test bridge failed' }));
        }
      });
      console.log(`Actual offline SQLite core: ${core.database}`);
    },
  };
}
