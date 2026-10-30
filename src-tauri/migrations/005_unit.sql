CREATE TABLE session_objectives(session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE, objective_id TEXT NOT NULL REFERENCES objectives(id), content_version TEXT NOT NULL);
INSERT INTO session_objectives SELECT session_id,'quadratic-convergence','quadratic-content-v2' FROM learning_sessions;
PRAGMA user_version = 5;
