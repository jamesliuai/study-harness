-- A learner goal and its plan revisions. Generated capabilities are ordinary objective rows with stable IDs, so tasks,
-- attempts, evidence and reviews keep the same execution path as the authored unit. Every plan change (a revision,
-- a skipped capability, an answered clarification) is a new frozen revision; capabilities are never rewritten.
CREATE TABLE goals(id TEXT PRIMARY KEY, data TEXT NOT NULL CHECK(json_valid(data)));
CREATE TABLE plans(goal_id TEXT NOT NULL REFERENCES goals(id) ON DELETE CASCADE, revision INTEGER NOT NULL CHECK(revision>0), data TEXT NOT NULL CHECK(json_valid(data)), PRIMARY KEY(goal_id,revision));
-- Each capability is studied in its own session, so evidence and review decisions stay scoped to one objective.
CREATE TABLE goal_sessions(session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE, goal_id TEXT NOT NULL REFERENCES goals(id) ON DELETE CASCADE);
CREATE TRIGGER frozen_goal BEFORE UPDATE ON goals BEGIN SELECT RAISE(ABORT,'A goal is frozen; revise its plan instead'); END;
CREATE TRIGGER frozen_plan BEFORE UPDATE ON plans BEGIN SELECT RAISE(ABORT,'Plan revisions are frozen; create a new revision'); END;
CREATE TRIGGER frozen_objective BEFORE UPDATE ON objectives BEGIN SELECT RAISE(ABORT,'Capabilities are frozen; a changed outcome is a new capability'); END;
PRAGMA user_version = 9;
