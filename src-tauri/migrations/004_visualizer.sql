-- Visual interaction records share the learning store and writer. They never grade or own responses.
CREATE TABLE visual_activities(id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE, learning_activity_id TEXT NOT NULL REFERENCES activities(id) ON DELETE CASCADE, template_id TEXT NOT NULL, state TEXT NOT NULL CHECK(json_valid(state)), UNIQUE(learning_activity_id,template_id));
CREATE TABLE visual_policies(activity_id TEXT NOT NULL REFERENCES visual_activities(id) ON DELETE CASCADE, revision INTEGER NOT NULL, data TEXT NOT NULL CHECK(json_valid(data)), PRIMARY KEY(activity_id,revision));
CREATE TABLE visual_operations(activity_id TEXT NOT NULL REFERENCES visual_activities(id) ON DELETE CASCADE, operation_key TEXT NOT NULL, method TEXT NOT NULL, fingerprint TEXT NOT NULL, result TEXT NOT NULL CHECK(json_valid(result)), PRIMARY KEY(activity_id,operation_key));
CREATE TABLE visual_exposures(id TEXT PRIMARY KEY, activity_id TEXT NOT NULL REFERENCES visual_activities(id) ON DELETE CASCADE, receipt_id TEXT NOT NULL UNIQUE, data TEXT NOT NULL CHECK(json_valid(data)));
CREATE TABLE visual_display_acks(exposure_id TEXT PRIMARY KEY REFERENCES visual_exposures(id) ON DELETE CASCADE, fingerprint TEXT NOT NULL, data TEXT NOT NULL CHECK(json_valid(data)));
CREATE TABLE visual_events(event_id TEXT PRIMARY KEY, activity_id TEXT NOT NULL REFERENCES visual_activities(id) ON DELETE CASCADE, sequence INTEGER NOT NULL, fingerprint TEXT NOT NULL, data TEXT NOT NULL CHECK(json_valid(data)), host_timestamp TEXT NOT NULL, UNIQUE(activity_id,sequence));
CREATE TABLE visual_artifacts(id TEXT PRIMARY KEY, activity_id TEXT NOT NULL REFERENCES visual_activities(id) ON DELETE CASCADE, digest TEXT NOT NULL, data TEXT NOT NULL CHECK(json_valid(data)));
CREATE TABLE visual_recoveries(id INTEGER PRIMARY KEY, exposure_id TEXT NOT NULL REFERENCES visual_exposures(id) ON DELETE CASCADE, data TEXT NOT NULL CHECK(json_valid(data)));
CREATE TABLE visual_attempt_snapshots(attempt_id TEXT PRIMARY KEY REFERENCES attempts(id) ON DELETE CASCADE, data TEXT NOT NULL CHECK(json_valid(data)));
CREATE TRIGGER visual_snapshot_frozen BEFORE UPDATE ON visual_attempt_snapshots BEGIN SELECT RAISE(ABORT,'Visual submission conditions are frozen'); END;
PRAGMA user_version = 4;
