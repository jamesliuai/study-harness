CREATE TABLE learning_sessions(session_id TEXT PRIMARY KEY REFERENCES sessions(id), data TEXT NOT NULL CHECK(json_valid(data)));
CREATE TABLE task_specs(activity_id TEXT PRIMARY KEY REFERENCES activities(id), data TEXT NOT NULL CHECK(json_valid(data)));
CREATE TABLE attempt_conditions(attempt_id TEXT PRIMARY KEY REFERENCES attempts(id), data TEXT NOT NULL CHECK(json_valid(data)));
CREATE TABLE learning_summaries(session_id TEXT PRIMARY KEY REFERENCES sessions(id), data TEXT NOT NULL CHECK(json_valid(data)));
CREATE TABLE disputes(id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(id), attempt_id TEXT NOT NULL REFERENCES attempts(id), reason TEXT NOT NULL, created_at TEXT NOT NULL, resolved_by TEXT REFERENCES assessments(id));
CREATE TABLE learning_events(id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(id), kind TEXT NOT NULL, created_at TEXT NOT NULL, data TEXT NOT NULL CHECK(json_valid(data)));
-- Frozen evidence is append-only. Corrections are new rows, never UPDATEs.
CREATE TRIGGER frozen_activity BEFORE UPDATE ON activities BEGIN SELECT RAISE(ABORT,'Activity is frozen'); END;
CREATE TRIGGER frozen_task BEFORE UPDATE ON task_specs BEGIN SELECT RAISE(ABORT,'Task specification is frozen'); END;
CREATE TRIGGER frozen_attempt BEFORE UPDATE ON attempts BEGIN SELECT RAISE(ABORT,'Attempt is immutable; create a revision'); END;
CREATE TRIGGER frozen_conditions BEFORE UPDATE ON attempt_conditions BEGIN SELECT RAISE(ABORT,'Submission conditions are frozen'); END;
CREATE TRIGGER frozen_assistance BEFORE UPDATE ON assistance BEGIN SELECT RAISE(ABORT,'Assistance is permanent'); END;
CREATE TRIGGER retain_assistance BEFORE DELETE ON assistance BEGIN SELECT RAISE(ABORT,'Assistance cannot be erased from an attempt'); END;
PRAGMA user_version = 3;
