# ct – Detailed Design Document (DDD)

**Version:** 1.0 (MVP, Read‑Only)
**Scope:** Daemon `ct-daemon`, CLI `ct`, REPL `ctrepl`
**Workspace Mode:** Supports **multi‑crate workspaces** (auto‑detected via `cargo metadata`)

---

## 1. Architecture Overview

### 1.1 Objectives (derived from PRD)

* Index a Rust **workspace (including multi‑crate)** into a **persistent symbol DB** (SQLite) + hot in‑memory maps (daemon).
* Serve **symbol‑centric** queries (`find`, `doc`, `ls`, `export`) with **lean, deterministic slices**, optional docs.
* Provide **progressive expansion** (`>` children, `<` parents) under a **hard character cap** with decision prompts.
* **Read‑only** MVP: no rename, no write ops. Include **file watching** and **benchmarking**.

### 1.2 Components

* **ct-daemon**: background server; indexes workspace, maintains in‑memory indices, answers IPC requests.
* **ct (CLI)**: one‑shot commands for agents/humans; autostarts daemon if needed.
* **ctrepl (REPL)**: interactive navigator (symbol tree feel), with tab completion.

### 1.3 High‑Level Data Flow

1. **Discovery**: `cargo metadata` → workspace crates graph (members, paths, package IDs, versions).
2. **Ingestion**: run `rustdoc --output-format json` **per crate** (workspace members only for MVP), parse, normalize, and stage into temp SQLite.
3. **Status pass**: light file reads to classify `implemented | unimplemented | todo` (scan selected spans; comments for TODO/FIXME).
4. **References pass (sparse)**: best‑effort cross‑refs for parent contexts (top‑N per symbol, bounded).
5. **Finalize**: compute `SymbolId` (BLAKE3 16B), create indices, **atomic swap** DB; daemon hot‑reloads maps.

### 1.4 Multi‑Crate Semantics

* **Scope of indexing**: all **workspace member crates** (as reported by `cargo metadata`). External deps are *not* deeply indexed in MVP; they appear only as **extern refs** in bundles (or are suppressed via `.ctignore`).
* **Canonical paths**: Global canonical path begins with **crate name**: `crate_name::mod::Type`. In REPL/CLI, a leading `crate::…` is resolved relative to the **current crate context**; otherwise you may specify `crate_name::…` to disambiguate.
* **Crate identity**: the `crates` table records `name`, `version`, `fingerprint`. Cross‑crate queries keep ordering stable with the global ranking and final tie on `symbol_id`.
* **Watcher coverage**: the daemon watches all member crate directories (excluding target/), debounced and batched.

---

## 2. Canonical Identity & Ordering

### 2.1 SymbolId

* **Definition**: `blake3(tool_fingerprint || def_path || kind || file_digest || span_start..span_end)`
* **Storage**: 16‑byte **BLOB** column (`symbols.symbol_id`), unique‑indexed.
* **Stability**: persists across restarts if code unchanged.

### 2.2 Deterministic Ordering

* `find` ranking: exact in REPL cwd > exact global > prefix > fuzzy.
* Ties: **public** before private → **workspace** crates before extern (if any) → **shorter path** → **earlier span** → **symbol\_id** (final total order).
* Lists (`find`, `ls`, expansion) are fully stable.

---

## 3. Slicing & Expansion

### 3.1 Default Slice

* Item header + **normalized signature** + **direct impls** + **fields/variants**.
* File digests are included for referenced files.

### 3.2 Docs Inclusion

* Default: **off**.
* `-d/--docs` applies to **root only**.
* `--docs=all` includes docs for **expanded** items as well.

### 3.3 Expansion Operators

* `>`: **children** (fields/variants/trait items/methods). Order: BFS by depth → path (lexicographic) → span\_start.
* `<`: **parents** (declaring module; best‑effort reference contexts via `references` table). BFS upward.
* **Impl chain**: with `--impl-parents`, `<` may traverse into an enclosing **impl**, then up to the **type**; for **trait impls**, the **trait definition is also fetched**.
* Operators **stack** (e.g., `>>>`, `<<`).

### 3.4 Hard Cap & Decision Prompt

* Hard cap `max_context_size` applies to the **serialized payload size** (characters).
* If exceeding cap, daemon returns a **decision envelope**: `continue` (truncate to cap), `abort`, or `full` (if `allow_full_context=true`).
* REPL surfaces the same decision interaction.

---

## 4. Indexing & Reindexing

### 4.1 Discovery (Multi‑Crate)

* Run `cargo metadata` from workspace root to enumerate **workspace members** with absolute paths, package IDs, and versions.
* Build a **crate graph** (members only for MVP). Persist crate fingerprints (`name@version + src digests + rustc hash + cfg snapshot`).

### 4.2 Ingestion Pipeline (Per Crate)

1. `rustdoc --output-format json` → parse items, signatures, docs (raw **Markdown** only, per MVP choice).
2. Normalize canonical paths to **`crate_name::…`**.
3. Stage rows into a **temp DB** (`symbols.sqlite.tmp`).

### 4.3 Status Detection

* Read the minimal text spans for functions to classify:

  * `unimplemented!()` → **unimplemented**.
  * `todo!()` or comments with `\bTODO\b|\bFIXME\b` → **todo**.
  * otherwise **implemented**.
* Implemented via light file reads; no HIR.

### 4.4 References (Sparse)

* Extract **top‑N** references per symbol (N configurable; default 16) to power best‑effort **parent contexts**.
* Skip exhaustive cross‑file analysis to keep indexing time bounded.

### 4.5 Atomic Swap & Hot Reload

* Build in temp DB with **WAL** + `synchronous=NORMAL`.
* `fsync` then rename temp → live DB (**atomic**).
* Daemon detects swap and **reopens**, rebuilding in‑memory maps.

### 4.6 Incremental Reindex

* **Watcher** detects changes → **debounce** \~**300 ms** (configurable), batch paths.
* Determine impacted **crates** from changed files (via crate root mapping).
* Re‑run ingestion only for **affected crates**; replace their rows; atomic swap.

### 4.7 Fingerprint Mismatch Policy

* On `rustc_hash`/`target`/`features`/`cfg` mismatch: **hard fail** with actionable error (no silent rebuild). User runs `ct reindex`.

---

## 5. Data Model (SQLite v1)

```sql
PRAGMA foreign_keys=ON;

CREATE TABLE meta (
  key TEXT PRIMARY KEY,
  val TEXT NOT NULL
);
-- keys: schema_version, tool_version, rustc_hash, features, target, created_at

CREATE TABLE crates (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  version TEXT,
  fingerprint TEXT NOT NULL
);

CREATE TABLE files (
  id INTEGER PRIMARY KEY,
  crate_id INTEGER NOT NULL REFERENCES crates(id),
  path TEXT NOT NULL,
  digest TEXT NOT NULL
);

CREATE TABLE symbols (
  id INTEGER PRIMARY KEY,               -- rowid
  symbol_id BLOB NOT NULL,              -- blake3 16 bytes
  crate_id INTEGER NOT NULL REFERENCES crates(id),
  file_id INTEGER NOT NULL REFERENCES files(id),
  path TEXT NOT NULL,                   -- canonical: crate_name::a::b::Type
  name TEXT NOT NULL,
  kind TEXT NOT NULL,                   -- module|struct|enum|trait|fn|method|field|variant|type_alias|const|static|impl
  visibility TEXT NOT NULL,             -- public|private
  signature TEXT NOT NULL,
  docs TEXT,                            -- raw Markdown; optional
  status TEXT NOT NULL,                 -- implemented|unimplemented|todo
  span_start INTEGER NOT NULL,
  span_end INTEGER NOT NULL,
  def_hash TEXT NOT NULL                -- hash(signature + span text)
);
CREATE UNIQUE INDEX ux_symbols_symbol_id ON symbols(symbol_id);
CREATE INDEX idx_symbols_name ON symbols(name COLLATE NOCASE);
CREATE INDEX idx_symbols_path ON symbols(path);
CREATE INDEX idx_symbols_kind ON symbols(kind);
CREATE INDEX idx_symbols_vis ON symbols(visibility);
CREATE INDEX idx_symbols_status ON symbols(status);

CREATE TABLE impls (
  id INTEGER PRIMARY KEY,
  for_path TEXT NOT NULL,
  trait_path TEXT,                      -- null for inherent impl
  file_id INTEGER NOT NULL REFERENCES files(id),
  line_start INTEGER NOT NULL,
  line_end INTEGER NOT NULL
);
CREATE INDEX idx_impls_for ON impls(for_path);

CREATE TABLE references (
  id INTEGER PRIMARY KEY,
  symbol_id INTEGER NOT NULL REFERENCES symbols(id),
  target_path TEXT NOT NULL,
  file_id INTEGER NOT NULL REFERENCES files(id),
  span_start INTEGER NOT NULL,
  span_end INTEGER NOT NULL
);
```

**Storage location**: `${XDG_CACHE_HOME}/ct/${WORKSPACE_FINGERPRINT}/symbols.sqlite` (or `.ct/` if configured).
**Reindexing** writes to `symbols.sqlite.tmp` then atomic rename.

**In‑memory maps (daemon)**:

* `path → id` (`HashMap<String, i64>`)
* `name → SmallVec<id>` (`HashMap<String, SmallVec<i64>>`)
* Optional fuzzy candidate cache (bounded by memory ceiling)

---

## 6. Search & Ranking

### 6.1 Query Types

* **By name** (case‑insensitive match on `name`).
* **By path** (case‑sensitive match on `path`), either `crate::…` (REPL context) or `crate_name::…`.

### 6.2 Fuzzy Logic (MVP)

* Construct a **candidate set** from name index (exact + prefix window, bounded e.g. 2,000).
* Score with a **ratio** (e.g., Rapidfuzz) over the candidate strings.
* Merge into ranking pipeline (after exact & prefix) with deterministic tiebreakers.

### 6.3 Visibility & Status Filters

* Global flags: `--vis public|private|all` (aliases `--public`, `--private`), `-u/--unimplemented`, `-t/--todo`.

### 6.4 Total Order

* Apply ranking stage, then the stable tie chain ending in `symbol_id` for absolute determinism across platforms/crates.

---

## 7. IPC Protocol (JSONL v1)

### 7.1 Transport

* **Unix domain socket** (Linux/macOS), **Windows named pipe** (Windows).
* **TCP fallback** (127.0.0.1 only) is opt‑in; gated by a **per‑session token** under `$XDG_RUNTIME_DIR/ct/<ws_fingerprint>/token`.

### 7.2 Framing & Versioning

*
* **JSONL** payloads (newline‑delimited); payload strings **escape newlines**.
* `protocol_version: 1` for MVP. Server advertises supported set `[1]`.

### 7.3 Envelopes

```json
{"ok":true, "request_id":"uuid", "protocol_version":1, "data":{...}, "truncated":false, "metrics":{"elapsed_ms":4, "bytes":2380}}
```

```json
{"ok":true, "request_id":"uuid", "protocol_version":1,
 "decision_required": {"reason":"OVER_MAX_CONTEXT", "content_len": 41237, "options": ["continue","abort","full"]}}
```

```json
{"ok":false, "request_id":"uuid", "protocol_version":1, "err":"explanation", "err_code":"INVALID_ARG"}
```

### 7.4 Requests (examples)

```json
{"cmd":"find","name":"State","kind":"struct","vis":"public","request_id":"..."}
{"cmd":"doc","path":"crate_name::util::State","include_docs":false,"request_id":"..."}
{"cmd":"ls","path":"crate::util","expansion":">>","impl_parents":false,"include_docs":false,"request_id":"..."}
{"cmd":"export","path":"crate_name::util::State","bundle":true,"expansion":"<>","include_docs":true,"vis":"all","request_id":"..."}
{"cmd":"reindex","features":["gpu"],"target":"x86_64-unknown-linux-gnu","request_id":"..."}
{"cmd":"status","vis":"private","todo":true,"request_id":"..."}
{"cmd":"diag","request_id":"..."}
{"cmd":"bench","queries":200,"duration":5,"request_id":"..."}
```

---

## 8. CLI / REPL Commands

### 8.1 CLI (`ct`)

```
ct find <name|path> [--kind <k>] [--vis public|private|all] [-u|--unimplemented] [-t|--todo] [--json|--pretty]
ct doc <path|name> [-d|--docs|--docs=all] [--vis ...] [-u] [-t]
ct ls  <path|name> [> ...] [< ...] [--impl-parents] [--vis ...] [-u] [-t]
ct export --bundle <path|name> [-d|--docs|--docs=all] [> ...] [< ...] [--impl-parents] [--vis ...] [-u] [-t] [--with-source]
ct reindex [--features ...] [--target ...]
ct status [--vis ...] [-u] [-t] [--json|--pretty]
ct diag [--json|--pretty]
ct bench [--queries N] [--warmup ms] [--duration s]
ct help [<command>] | --help
```

**Per‑command caps**: `--max-size` overrides global `max_context_size`.
**Case handling**: `find` name is **case‑insensitive**; path is case‑sensitive.
**Status**: Shows implementation status **only** - counts + bounded lists (respects `--vis`, `-u`, `-t`, `--json|--pretty`).
**Diag**: Shows ops/daemon/DB diagnostics (db_path, schema_version, tool_version, etc.).
**Help**: `ct open` is documented behind `--allow-open` note (not in MVP).

### 8.2 REPL (`ctrepl`)

* Minimal commands: `cd`, `ls`, `doc`, `find`, `export`, `quit`; tab completion; `!cmd` for shell.
* REPL prompt reflects crate context: `(ct <crate_name>::path)>`.
* Over‑max → interactive **decision** dialog (same semantics as CLI/daemon).

### 8.3 Exit Codes (for agents)

* `0` ok, `2` invalid args, `3` over‑max decision required, `4` daemon unavailable, `5` index mismatch, `6` internal error.

---

## 9. Watcher & Daemon Lifecycle

### 9.1 Watch Strategy

* Watch all **member crate roots**; ignore `target/` and configured globs.
* **Debounce** default **300 ms**; batch/coalesce; crate‑level re‑ingest.

### 9.2 Daemon Autostart & Isolation

* **Per‑workspace** daemon; socket/pipe path includes **workspace fingerprint**.
* Local IPC only by default; TCP fallback opt‑in with token & idle timeouts.

### 9.3 Security

* **Read‑only** within `workspace_allow`.
* Canonicalize & resolve symlinks; reject escapes.
* Redact file contents in logs; optional path redaction.

---

## 10. Bundles & Output

### 10.1 Bundle Shape (excerpt)

```json
{
  "symbol": { "symbol_id":"…", "path":"crate_name::util::State", "kind":"struct", "signature":"…", "docs":"…" },
  "children": [ /* ordered */ ],
  "extern_refs": [ /* extern crate types referenced, unless .ctignore */ ],
  "impl_ranges": [
    {"file":"src/util/state.rs","file_digest":"blake3:…","line_start":42,"line_end":118},
    {"file":"src/util/state_ext.rs","file_digest":"blake3:…","line_start":10,"line_end":57}
  ],
  "order":"bfs",
  "invariants": {"range_1_based_inclusive":true}
}
```

### 10.2 With Source

* `--with-source` embeds **source snippets per item** up to a cap `bundle_source_cap` (default **3000 chars**, configurable).
* Interacts with global/per‑command caps predictably (strict serialized size enforcement).

### 10.3 Pretty vs JSON

* Default: compact **JSON** (agent‑friendly).
* `--pretty` for human‑friendly output; docs are rendered client‑side if needed.

### 10.4 Command Output Formats

#### `ct status` Output (Implementation Status Only)
```json
{
  "counts": {
    "total": 2847,
    "implemented": 2612,
    "unimplemented": 123,
    "todo": 112
  },
  "items": [
    {"path": "crate_name::module::function", "status": "unimplemented", "kind": "fn"},
    {"path": "crate_name::util::process_data", "status": "todo", "kind": "fn"}
  ]
}
```

#### `ct diag` Output (Ops/Daemon/DB Diagnostics)
```json
{
  "db_path": "/home/user/.cache/ct/workspace_fingerprint/symbols.sqlite",
  "schema_version": "1",
  "tool_version": "0.1.0",
  "protocol_versions_supported": [1],
  "workspace_root": "/path/to/workspace",
  "workspace_fingerprint": "blake3:abcd1234...",
  "crate_count": 5,
  "file_count": 342,
  "symbol_count": 12847,
  "mem_footprint_bytes": 134217728,
  "last_index_duration_ms": 3421,
  "index_timestamp": "2025-01-15T10:23:45Z",
  "rustc_hash": "sha256:ef5678...",
  "features": ["gpu", "async"],
  "target": "x86_64-unknown-linux-gnu",
  "daemon_hot": true,
  "transport": "unix"
}
```

---

## 11. Configuration (`ct.toml` excerpt)

```toml
# Transport & daemon
transport = "auto"                # auto|unix|pipe|tcp (tcp disabled unless explicitly set)
autostart = true
socket_path = "/tmp/ctd.sock"
pipe_name = "\\\\.\\pipe\\ctd"
tcp_addr = "127.0.0.1:48732"
allow_full_context = false         # controls whether "full" decision option is offered

# Workspace
workspace_allow = ["/abs/path/to/ws"]

# Slicing
max_context_size = 16000           # hard cap (characters)

# Status listing
max_list = 200                     # default max items listed by `ct status`

# Bundle
bundle_source_cap = 3000           # per-item source cap when --with-source

# Index DB
db_dir = "${XDG_CACHE_HOME}/ct/${WORKSPACE_FINGERPRINT}"
db_file = "symbols.sqlite"

# References
references_top_n = 16

# Fuzzy / memory ceiling
max_mem_mb = 512                   # disable fuzzy if exceeded

# Bench
bench_queries = 200
bench_duration_s = 5
```

---

## 12. Performance Targets & Bench

### 12.1 Targets (warm daemon)

* `find` name: P50 1–10 ms, P99 < 20 ms
* `doc` path: P50 1–5 ms
* `ls` path (shallow): P50 3–15 ms
* `export` bundle (to cap): P50 5–50 ms, P99 < 120 ms
* Memory: O(#symbols) for maps; typical 50–250 MB on large multi‑crate workspaces

### 12.2 `ct bench`

* Measures: initial index time (cold), incremental after touching N files (via watcher simulation), and mixed query latency distributions.
* Output: human summary + JSON with: query latency distributions (P50/P90/P99), throughput metrics, and benchmark configuration used.

---

## 13. Testing Strategy (Multi‑Crate)

### 13.1 Test Assets

* **Multi‑crate test workspace** under `tests/fixtures/` with:

  * Cross‑crate deps and re‑exports
  * Generics, traits, inherent & trait impls
  * `unimplemented!()`, `todo!()` and `TODO/FIXME` comments

### 13.2 Unit / Property

* Path normalization & canonicalization (`crate::` vs `crate_name::`).
* Ranking stability & total order invariants (incl. final tie on `symbol_id`).
* Expansion invariants (BFS order; parent traversal semantics with `--impl-parents`).
* Status classification correctness.

### 13.3 Integration

* Daemon IPC handshake; autostart.
* Atomic swap correctness under concurrent reads.
* Watcher → debounce → crate‑level incremental reindex.
* Multi‑crate: find/doc/ls/export across crates; .ctignore crate semantics.

### 13.4 Load / Soak

* 10 concurrent clients @ 100 rps mixed queries; assert P99 SLAs; no unbounded memory growth.

---

## 14. Acceptance Criteria (Multi‑Crate)

* `ct-daemon --idx .` discovers **all workspace crates** via `cargo metadata` and builds the DB.
* `ct status` lists (bounded by `max_list`) unimplemented/todo across **all member crates** (implementation status only).
* `ct status --ops` (deprecated): emits deprecation warning and redirects to `ct diag` output for back-compat.
* `ct find State` returns ranked matches possibly spanning **multiple crates**; order is deterministic.
* `ct doc crate_name::util::State` returns signature; `ct ls crate::util >` works from REPL crate context.
* `ct export --bundle crate_name::util::State -d` returns bundle with `impl_ranges` + file digests; respects `.ctignore` for extern crates.
* Over‑max returns decision prompt; `--docs=all` and `--with-source` behave predictably under caps.

---

## 15. Failure Modes & Safeguards

* **Fingerprint mismatch** → error code 5; actionable message to run `ct reindex` (no silent rebuild).
* **Oversize** → decision envelope; CLI non‑zero exit if user doesn’t choose.
* **Watcher churn** → debounce & batch; if backlog persists, log rate‑limit and coalesce.
* **Re‑exports/aliasing** → risk of duplicate/ambiguous parents; normalize parent as defining module (future `reexports` table in a migration).

---

## 16. Migration Plan

* **Schema v1** as defined.
* **Command separation** (v1.1): `ct status` focuses on implementation status; new `ct diag` for ops/daemon/DB diagnostics. Back-compat: `ct status --ops` redirects to `ct diag` with deprecation warning.
* Future migrations:

  * `reexports(symbol_id, parent_id, alias)` for de‑duping parents.
  * FTS5 tables if fuzzy/docs search is promoted.
  * Protocol v2: length‑prefixed frames.

---

## 17. Implementation Notes

* Use **WAL** + `synchronous=NORMAL`; set `journal_size_limit`; prefer `temp_store=MEMORY` during build.
* Avoid loading full docs for non‑`--docs` queries; keep hot maps **lean** (names/paths/ids).
* Memory ceiling gates fuzzy candidate construction and n‑gram caches.
* On Windows, ensure named pipe security descriptor restricts to current user.

---

## 18. Open Telemetry (Optional, MVP‑quiet)

* Minimal metrics in response envelopes (`elapsed_ms`, `bytes`).
* Optional daemon counters (behind env flag): index durations, watcher events, cache hit rates.

---

## 19. Known Limitations (MVP)

* No correctness guarantees across all `cfg`/macro expansions (Phase 2 via RA/HIR).
* No implementors/who‑uses/call graph.
* Extern crates not deeply indexed (workspace‑only), except for names in `extern_refs`.

---

## 20. Improvement Notes (critical viewpoints)

* **Clean command separation**: `ct status` now focuses on implementation status only; `ct diag` provides ops/daemon/DB diagnostics. This prevents JSON contract brittleness and enables cleaner agent tooling.
* **JSONL framing** is adequate now; length‑prefixing should not slip past v2—prevents edge‑case escaping bugs and simplifies streaming parsers.
* **Sparse references** keep parents fast, but cut edges may hide important contexts; expose `references_top_n` per command (not just config) if agents hit ambiguity.
* **Multi‑crate canonicalization**: be strict. Prefer `crate_name::…` globally; treat `crate::…` only as a REPL convenience to avoid cross‑crate confusion.
