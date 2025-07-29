pub const CURRENT_VERSION: u32 = 1;

pub const V1_SCHEMA: &str = r#"
PRAGMA foreign_keys=ON;

CREATE TABLE IF NOT EXISTS meta (
  key TEXT PRIMARY KEY,
  val TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS crates (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  version TEXT,
  fingerprint TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS files (
  id INTEGER PRIMARY KEY,
  crate_id INTEGER NOT NULL REFERENCES crates(id),
  path TEXT NOT NULL,
  digest TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS symbols (
  id INTEGER PRIMARY KEY,
  symbol_id BLOB NOT NULL,
  crate_id INTEGER NOT NULL REFERENCES crates(id),
  file_id INTEGER NOT NULL REFERENCES files(id),
  path TEXT NOT NULL,
  name TEXT NOT NULL,
  kind TEXT NOT NULL,
  visibility TEXT NOT NULL,
  signature TEXT NOT NULL,
  docs TEXT,
  status TEXT NOT NULL,
  span_start INTEGER NOT NULL,
  span_end INTEGER NOT NULL,
  def_hash TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS ux_symbols_symbol_id ON symbols(symbol_id);
CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name COLLATE NOCASE);
CREATE INDEX IF NOT EXISTS idx_symbols_path ON symbols(path);
CREATE INDEX IF NOT EXISTS idx_symbols_kind ON symbols(kind);
CREATE INDEX IF NOT EXISTS idx_symbols_vis ON symbols(visibility);
CREATE INDEX IF NOT EXISTS idx_symbols_status ON symbols(status);

CREATE TABLE IF NOT EXISTS impls (
  id INTEGER PRIMARY KEY,
  for_path TEXT NOT NULL,
  trait_path TEXT,
  file_id INTEGER NOT NULL REFERENCES files(id),
  line_start INTEGER NOT NULL,
  line_end INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_impls_for ON impls(for_path);

CREATE TABLE IF NOT EXISTS symbol_references (
  id INTEGER PRIMARY KEY,
  symbol_id INTEGER NOT NULL REFERENCES symbols(id),
  target_path TEXT NOT NULL,
  file_id INTEGER NOT NULL REFERENCES files(id),
  span_start INTEGER NOT NULL,
  span_end INTEGER NOT NULL
);
"#;