CREATE TABLE IF NOT EXISTS samples (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  tool TEXT NOT NULL CHECK (tool IN ('coordinate', 'analyze', 'policy_check')),
  taken_at TEXT NOT NULL,
  repo_path TEXT NOT NULL,
  commit_sha TEXT,
  input_json TEXT NOT NULL,
  output_json TEXT NOT NULL,
  exit_code INTEGER,
  summary_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_samples_repo_tool_time
  ON samples (repo_path, tool, taken_at);
