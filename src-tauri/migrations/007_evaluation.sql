-- Held-out material is separate from tutoring sources, FTS and ordinary workspace payloads.
CREATE TABLE evaluation_pairs(id TEXT PRIMARY KEY, data TEXT NOT NULL CHECK(json_valid(data)));
CREATE TABLE evaluation_topics(id TEXT PRIMARY KEY, pair_id TEXT NOT NULL REFERENCES evaluation_pairs(id) ON DELETE CASCADE, data TEXT NOT NULL CHECK(json_valid(data)));
CREATE TABLE evaluation_links(topic_id TEXT PRIMARY KEY REFERENCES evaluation_topics(id) ON DELETE CASCADE, session_id TEXT NOT NULL UNIQUE REFERENCES sessions(id) ON DELETE CASCADE);
CREATE TABLE evaluation_events(id TEXT PRIMARY KEY, topic_id TEXT NOT NULL REFERENCES evaluation_topics(id) ON DELETE CASCADE, data TEXT NOT NULL CHECK(json_valid(data)));
CREATE TABLE evaluation_timers(topic_id TEXT PRIMARY KEY REFERENCES evaluation_topics(id) ON DELETE CASCADE, data TEXT NOT NULL CHECK(json_valid(data)));
CREATE TABLE evaluation_lessons(topic_id TEXT PRIMARY KEY REFERENCES evaluation_topics(id) ON DELETE CASCADE, data TEXT NOT NULL CHECK(json_valid(data)));
CREATE TABLE evaluation_exams(topic_id TEXT PRIMARY KEY REFERENCES evaluation_topics(id) ON DELETE CASCADE, data TEXT NOT NULL CHECK(json_valid(data)));
CREATE TABLE evaluation_responses(topic_id TEXT PRIMARY KEY REFERENCES evaluation_topics(id) ON DELETE CASCADE, data TEXT NOT NULL CHECK(json_valid(data)));
CREATE TABLE evaluation_scores(id TEXT PRIMARY KEY, topic_id TEXT NOT NULL REFERENCES evaluation_topics(id) ON DELETE CASCADE, data TEXT NOT NULL CHECK(json_valid(data)));
CREATE TRIGGER evaluation_pair_frozen BEFORE UPDATE ON evaluation_pairs BEGIN SELECT RAISE(ABORT,'Comparison registration is frozen'); END;
CREATE TRIGGER evaluation_topic_frozen BEFORE UPDATE ON evaluation_topics BEGIN SELECT RAISE(ABORT,'Held-out material is frozen'); END;
CREATE TRIGGER evaluation_response_frozen BEFORE UPDATE ON evaluation_responses BEGIN SELECT RAISE(ABORT,'Held-out response is frozen'); END;
CREATE TRIGGER evaluation_score_frozen BEFORE UPDATE ON evaluation_scores BEGIN SELECT RAISE(ABORT,'Append a new score record'); END;
PRAGMA user_version = 7;
