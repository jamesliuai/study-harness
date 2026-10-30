-- Keep recognized speech distinct from an editable draft through restart and retries.
ALTER TABLE attempts ADD COLUMN transcript_id TEXT;
CREATE TABLE pending_transcripts (
    session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
    transcript_id TEXT NOT NULL UNIQUE,
    data TEXT NOT NULL CHECK(json_valid(data))
);
PRAGMA user_version = 2;
