import { useState } from 'react';
import privacy from '../../docs/PRIVACY.md?raw';
import recovery from '../../docs/RECOVERY.md?raw';
import support from '../../docs/SUPPORT.md?raw';
import evaluation from '../../docs/EVALUATION-PROTOCOL.md?raw';
const guides = {
  Privacy: privacy,
  'Backup and recovery': recovery,
  Support: support,
  'Personal comparison protocol': evaluation,
};
export function LocalGuide() {
  const [selected, setSelected] = useState<keyof typeof guides>('Privacy');
  return (
    <details>
      <summary>Local privacy, recovery and support guides</summary>
      <label htmlFor="local-guide">Guide</label>
      <select
        id="local-guide"
        value={selected}
        onChange={(e) => setSelected(e.target.value as keyof typeof guides)}
      >
        {Object.keys(guides).map((name) => (
          <option key={name}>{name}</option>
        ))}
      </select>
      <pre className="local-guide" role="region" tabIndex={0} aria-label={selected}>
        {guides[selected]}
      </pre>
    </details>
  );
}
