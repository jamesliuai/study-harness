import { useState } from 'react';
import { command } from '../api';
import type { DeletionPreview, Workspace } from '../generated/contracts';

export function DataControls({
  workspace,
  refresh,
}: {
  workspace: Workspace;
  refresh: () => Promise<void>;
}) {
  const [password, setPassword] = useState(''),
    [busy, setBusy] = useState(false),
    [notice, setNotice] = useState('');
  const [scope, setScope] = useState('session'),
    [source, setSource] = useState(''),
    [preview, setPreview] = useState<DeletionPreview | null>(null),
    [confirmation, setConfirmation] = useState('');
  const act = async (fn: () => Promise<void>) => {
    setBusy(true);
    setNotice('');
    try {
      await fn();
      await refresh();
    } catch (e) {
      setNotice(String(e));
    } finally {
      setBusy(false);
      setPassword('');
    }
  };
  return (
    <section aria-labelledby="ownership-title">
      <h3 id="ownership-title">Your data and recovery</h3>
      <p className="small">
        Archives contain your original responses, sources, citations, usage, and visual artifacts
        with their exact content versions. Credentials and raw audio are excluded. A portable export
        is unencrypted; keep it private. Protected backups require your passphrase and cannot be
        recovered without it.
      </p>
      <label>
        Backup passphrase{' '}
        <input
          type="password"
          autoComplete="new-password"
          value={password}
          maxLength={1024}
          placeholder="At least 12 characters, for a protected backup"
          onChange={(e) => setPassword(e.target.value)}
        />
      </label>
      <div className="actions">
        <button
          disabled={busy}
          onClick={() =>
            void act(async () => {
              const path = await command<string | null>('export_data', {
                protected: false,
                passphrase: '',
              });
              if (path) setNotice(`Portable archive saved to ${path}`);
            })
          }
        >
          Export portable archive
        </button>
        <button
          disabled={busy || password.length < 12}
          title={
            password.length < 12
              ? 'Enter a passphrase of at least 12 characters first'
              : 'Encrypted with your passphrase; it cannot be recovered without it'
          }
          onClick={() =>
            void act(async () => {
              const path = await command<string | null>('export_data', {
                protected: true,
                passphrase: password,
              });
              if (path) setNotice(`Protected backup saved to ${path}`);
            })
          }
        >
          Save protected backup
        </button>
      </div>
      <p className="small">
        Restore accepts this release’s known content versions and a fresh profile of the same mode.
        It checks hashes, database schema, integrity, references, and visual records before
        adoption. An archive remains a user-supplied record, not a certificate of learning or
        review.
      </p>
      <p className="small">
        To open a fresh profile, quit the app and launch its executable with{' '}
        <code>--profile restore-1</code>. Choose a new name for each fresh profile. Enter the
        passphrase above for a protected backup.
      </p>
      <button
        disabled={busy}
        onClick={() =>
          void act(async () => {
            if (await command<boolean>('restore_data', { passphrase: password || null })) {
              setNotice(
                'Archive restored. Imported study records preserve their original evidence and simulation labels.',
              );
              setPreview(null);
            }
          })
        }
      >
        Restore selected archive
      </button>
      <details>
        <summary>Delete records and derived data</summary>
        <p className="small">
          This also removes app-managed migration backups, which can contain deleted records.
          Exported files you saved elsewhere remain under your control.
        </p>
        <label>
          Delete scope{' '}
          <select
            value={scope}
            disabled={busy}
            onChange={(e) => {
              setScope(e.target.value);
              setPreview(null);
              setConfirmation('');
            }}
          >
            <option value="session">Current session</option>
            <option value="source">A source and its dependent sessions</option>
          </select>
        </label>
        {scope === 'source' ? (
          <label htmlFor="delete-source">
            Source{' '}
            <select
              id="delete-source"
              value={source}
              onChange={(e) => {
                setSource(e.target.value);
                setPreview(null);
                setConfirmation('');
              }}
            >
              <option value="">Choose a source</option>
              {workspace.sources.documents.map((d) => (
                <option key={d.id} value={d.id}>
                  {d.title}
                </option>
              ))}
            </select>
          </label>
        ) : (
          <p>{workspace.sessions.find((s) => s.id === workspace.active_session_id)?.title}</p>
        )}
        <button
          disabled={busy || (scope === 'source' && !source)}
          onClick={() =>
            void act(async () => {
              setPreview(
                await command<DeletionPreview>('deletion_preview', {
                  scope,
                  targetId: scope === 'session' ? workspace.active_session_id : source,
                }),
              );
            })
          }
        >
          Preview deletion
        </button>
        {preview ? (
          <div>
            <p>{preview.notice}</p>
            <p>
              {preview.session_ids.length} sessions and {preview.attempt_count} attempts will be
              deleted.
            </p>
            <label>
              Type DELETE to confirm{' '}
              <input
                value={confirmation}
                autoComplete="off"
                onChange={(e) => setConfirmation(e.target.value)}
              />
            </label>
            <button
              className="danger"
              disabled={busy || confirmation !== 'DELETE'}
              onClick={() =>
                void act(async () => {
                  await command('delete_records', {
                    scope: preview.scope,
                    targetId: preview.target_id,
                    confirmation,
                  });
                  setPreview(null);
                  setConfirmation('');
                  setNotice('Selected records and derived data were deleted.');
                })
              }
            >
              Delete these records
            </button>
          </div>
        ) : null}
      </details>
      <p role="status">{busy ? 'Working with local data…' : notice}</p>
    </section>
  );
}
