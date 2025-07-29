# ct — Product Requirements (Revised v0.2, MVP Read‑Only)

**Components:** daemon `ct-daemon`, CLI `ct`, REPL `ctrepl`

---

## 0) Snapshot

**Problem.** Agents and humans waste tokens/time skimming entire files to understand/modify a small set of symbols.

**Solution.** `ct` indexes a Rust workspace (including multi-crate workspaces) into a persistent symbol database and answers **symbol‑centric queries** with tight, deterministic slices (code/signatures, optional docs). It supports progressive expansion and bundle export for precise agent context.

**MVP scope.** Index → find → doc → ls → export bundle; REPL; daemon; `.ctignore`; **read‑only** (no rename). **File watching** and **benchmarking** are in scope.

**Outside MVP.** Call graph, implementors, who‑uses; HIR backend; editor open; MCP server; rename.

---

## 1) Goals & Non‑Goals

### Goals

* Query by **name** or **canonical path** for modules/types/functions.
* Return **lean slices** (item + signature + direct impls + fields/variants), with optional docs.
* Progressive **expansion** using operators with a **hard character cap**.
* **REPL** for humans and **one‑shot CLI** for agents.
* Persist to **SQLite** (file extension: `.sqlite`) with a hot **in‑memory index** in the daemon.
* **Visibility filtering**: public‑only, private‑only, or all.
* **Status filtering**: `--unimplemented/-u` and `--todo/-t`.
* **File watching** for incremental updates (MVP).
* **Benchmarking** command (MVP).

### Non‑Goals (MVP)

* Correctness across all `cfg`/macro expansions (Phase 2 via rust‑analyzer HIR).
* Call graph and who‑uses beyond best‑effort parents.
* Rename and any write operations (keep read‑only).

---

## 2) Personas & Primary Flows

**Agent.** Needs minimal, correct, **deterministic** slices with stable IDs and fast JSON responses.

**Human (power user).** Wants a REPL that feels like navigating a **symbol tree**, plus export for review.

**Primary flows**

* "Find `State` and export the bundle for implementation."
* "Show docs/signature of `crate::util::State` quickly."
* "Expand children/parents just enough under a size limit."
* "List unimplemented/todo functions to plan work."

---

## 3) Modes of Operation

* **One‑shot CLI (`ct`)**: `find`, `doc`, `ls`, `export`, `reindex`, `status`, `bench`, `help`.
* **REPL (`ctrepl`)**: minimal set `cd/ls/doc/find/export/quit`, tab completion; `!cmd` escape. No soft cap, no open.
* **Daemon (`ct-daemon`)**: background server that indexes and answers queries over local IPC.

---

## 4) Setup & Installation

### Option 1: System-wide Installation (PATH)

* Download the `ct`, `ct-daemon`, and `ctrepl` binaries
* Place them in a directory on your PATH (e.g., `/usr/local/bin`, `~/.local/bin`)
* Navigate to your Rust workspace root
* Run `ct-daemon --idx .` to start the daemon and index the workspace
* Use `ct` commands from anywhere

### Option 2: Docker Container Setup

**Dockerfile:**
```dockerfile
# Add ct binaries to the image
COPY ct /usr/local/bin/ct
COPY ct-daemon /usr/local/bin/ct-daemon
COPY ctrepl /usr/local/bin/ctrepl
RUN chmod +x /usr/local/bin/ct*

# Optional: pre-index the workspace during build
# RUN cd /workspace && ct-daemon --idx . --once
```

**run.sh (if using an entrypoint script):**
```bash
#!/bin/bash
# Start the ct daemon in background if not running
if ! pgrep -x "ct-daemon" > /dev/null; then
    ct-daemon --idx /workspace &
    sleep 2  # Give daemon time to start
fi

# Run your main application or shell
exec "$@"
```

### Option 3: Workspace-local Installation

* Place `ct`, `ct-daemon`, and `ctrepl` binaries in your workspace root
* Run `./ct-daemon --idx .` from the workspace root
* Use `./ct` for queries

### Initial Setup

Regardless of installation method:
1. Start the daemon: `ct-daemon --idx <path-to-rust-workspace>`
2. Wait for initial indexing (check with `ct status`)
3. Begin querying with `ct find`, `ct doc`, etc.

---

## 5) Slicing & Expansion Model

### Canonical identity & paths

* Canonical path uses Rust module syntax (no filesystem): e.g., `crate::util::types::State`.
* **SymbolId** = `blake3(tool_fingerprint || def_path || kind || file_digest || span_start..span_end)` truncated to 16 bytes (hex). Stable across restarts if code unchanged.
* **Line ranges** are **1‑based, inclusive**, normalized to LF.

### Default export unit

* **Symbol slice**: item header + normalized signature + direct impls + fields/variants.
* Include **file digest** for every file referenced by a slice.

### Docs inclusion

* Off by default, enable with `-d/--docs`.

### Expansion operators & order

* `>` **children** (fields/variants/trait items/methods). BFS by depth, then stable sort: rank → path (lexicographic) → span\_start.
* `<` **parents** (context above a definition). Parents include the **declaring module** and best‑effort **reference contexts** (see below). BFS upward.
* **Parents‑of‑parents via impl chain**: when `--impl-parents` is set, `<` may traverse *into the enclosing impl*, then up to the **type being implemented**, allowing you to fetch its definition as a parent‑of‑parent.
* Operators **stack** (`>>>`, `<<` etc.). Expansion always respects the **hard cap** `max_context_size` (characters). The root item's header and signature are always included.

### Parents semantics (clarified)

* *Parents* are "context above the definition". Examples:

  * If a **struct** is used in a **function**, the function is treated as a parent context (best‑effort via references table; may be incomplete in MVP).
  * Within **impl blocks**, functions have a parent of the **impl**, whose parent is the **type** being implemented. With `--impl-parents`, `<<` can reach the type definition from a method.

### Over‑max behavior (no streaming cancel)

* If an operation would exceed `max_context_size`, the daemon **does not stream partials**. Instead it returns a **decision prompt**:

  * Reports `content_len` (estimated) and presents **options**: `continue` (send truncated to cap), `abort`, or `full`.
  * The **`full`** option is shown only if allowed by config (`allow_full_context=true`).
  * Clients may re‑issue the same request with `decision:"continue"|"abort"|"full"`.

---

## 6) Determinism & Ordering (Normative)

* **Ranking for `find`**: exact‑in‑REPL‑cwd > exact‑global > prefix > fuzzy. Ties broken by: `pub` items first → workspace crate before extern → shorter path → earlier span.
* All lists (`find`, `ls`, expansion) use a **stable total order**: `rank` → `path` → `span_start`.

---

## 7) Functional Requirements

### Daemon (`ct-daemon`)

* `ct-daemon --idx <workspace> [--features ...] [--target ...] [--transport auto|unix|pipe|tcp]`
* Automatically detects and indexes **all crates** in a Cargo workspace (via `cargo metadata`)
* Builds/refreshes **SQLite** DB (`symbols.sqlite`) merging symbols from all workspace crates
* Serves **JSONL** frames over IPC (see Protocol). Autostarts on demand (configurable).
* **File watching (MVP)**: watch workspace; on change, perform **incremental reindex** (changed files/crates only) with atomic swap.

### CLI (`ct`)

```bash
ct find <name|path> [--kind <k>] [--vis public|private|all] [-u|--unimplemented] [-t|--todo] [--json|--pretty]
ct doc <path|name> [-d|--docs] [--vis ...] [-u] [-t]
ct ls <path|name> [> ...] [< ...] [--impl-parents] [--vis ...] [-u] [-t]
ct export --bundle <path|name> [-d|--docs] [> ...] [< ...] [--impl-parents] [--vis ...] [-u] [-t]
ct reindex [--features ...] [--target ...]
ct status [--vis ...] [-u] [-t]                  # implementation status only
ct bench [--queries N] [--warmup ms] [--duration s]
ct help [<command>] | --help
```

* **Global flags**: `--vis public|private|all` (aliases: `--public`, `--private`), `-u/--unimplemented`, `-t/--todo`.
* `ct status` outputs **implementation status** only: implemented / `unimplemented!()` / `todo!()` / `TODO|FIXME` counts and (optionally) lists.

### REPL (`ctrepl`)

* Minimal: `cd`, `ls`, `doc`, `find`, `export`, `quit`; tab completion; `!cmd` for shell. No soft‑cap logic.

### `.ctignore`

* At workspace root. One pattern per line. `#` for comments. Examples:

```gitignore
# Ignore entire crates
serde
serde < 2
vendor/**

# Ignore specific modules using Rust paths
blah::util::types
my_crate::internal::legacy
```

* **Semantics**: 
  * Crate names (optionally with a **version upper bound** style `name < [version]`)
  * Module paths using Rust syntax (e.g., `crate_name::module::submodule`)
  * Simple path globs for file system paths
  * Ignored crates/modules are not deeply expanded in bundles; only names/signatures appear.

---

## 8) Data Model & Storage (v1)

### SQLite schema (normalized)

```sql
PRAGMA foreign_keys=ON;

CREATE TABLE meta (
  key TEXT PRIMARY KEY,
  val TEXT NOT NULL
); -- keys: schema_version, tool_version, rustc_hash, features, target, created_at

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
  symbol_id TEXT NOT NULL,              -- blake3 hex(16 bytes)
  crate_id INTEGER NOT NULL REFERENCES crates(id),
  file_id INTEGER NOT NULL REFERENCES files(id),
  path TEXT NOT NULL,                   -- canonical: crate::a::b::Type
  name TEXT NOT NULL,
  kind TEXT NOT NULL,                   -- module|struct|enum|trait|fn|method|field|variant|type_alias|const|static|impl
  visibility TEXT NOT NULL,             -- public|private
  signature TEXT NOT NULL,
  docs TEXT,                            -- optional
  status TEXT NOT NULL,                 -- implemented|unimplemented|todo
  span_start INTEGER NOT NULL,
  span_end INTEGER NOT NULL,
  def_hash TEXT NOT NULL                -- hash(signature + span text)
);
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

-- Optional, sparse MVP to power parents via references (best‑effort)
CREATE TABLE references (
  id INTEGER PRIMARY KEY,
  symbol_id INTEGER NOT NULL REFERENCES symbols(id),
  target_path TEXT NOT NULL,
  file_id INTEGER NOT NULL REFERENCES files(id),
  span_start INTEGER NOT NULL,
  span_end INTEGER NOT NULL
);
```

### Storage & location

* DB filename: `symbols.sqlite` under `$XDG_CACHE_HOME/ct/<workspace_fingerprint>/` (default) or workspace `.ct/` directory if configured.
* Atomic reindex writes to `symbols.sqlite.tmp` and performs a rename swap.

### In‑memory index (daemon)

* `path → id` (`HashMap<String, i64>`)
* `name → SmallVec<id>` (`HashMap<String, SmallVec<i64>>`)
* Optional n‑gram index for fuzzy, disabled if memory ceiling exceeded.

---

## 9) Protocol (JSON over local IPC)

### Transports

* **Unix domain socket** on Linux/macOS (default).
* **Windows named pipe** on Windows.
* **TCP** fallback is **optional** and **disabled by default**; intended for rare environments where UDS/pipes are unavailable (e.g., certain containers/WSL). Enable via config or feature flag.

### Framing & envelope

* **JSONL** (newline‑delimited). Payload strings always escape newlines.
* Every request includes `request_id` and `protocol_version`.
* Success envelope:

```json
{
  "ok": true,
  "request_id": "uuid",
  "protocol_version": 1,
  "data": {...},
  "truncated": false,
  "metrics": {
    "elapsed_ms": 4,
    "bytes": 2380
  }
}
```

* Limit/decision envelope (when over cap):

```json
{
  "ok": true,
  "request_id": "uuid",
  "protocol_version": 1,
  "decision_required": {
    "reason": "OVER_MAX_CONTEXT",
    "content_len": 41237,
    "options": ["continue", "abort", "full"]
  }
}
```

* Error envelope uses typed codes:

```json
{
  "ok": false,
  "request_id": "uuid",
  "protocol_version": 1,
  "err": "explanation",
  "err_code": "INVALID_ARG"
}
```

### Requests (examples)

```json
{"cmd": "find", "name": "State", "kind": "struct", "vis": "public", "request_id": "..."}
{"cmd": "doc", "path": "crate::util::State", "include_docs": false, "request_id": "..."}
{"cmd": "ls", "path": "crate::util", "expansion": ">>", "impl_parents": false, "include_docs": false, "request_id": "..."}
{"cmd": "export", "path": "crate::util::State", "bundle": true, "expansion": "<>", "include_docs": true, "vis": "all", "request_id": "..."}
{"cmd": "reindex", "features": ["gpu"], "target": "x86_64-unknown-linux-gnu", "request_id": "..."}
{"cmd": "status", "vis": "private", "todo": true, "request_id": "..."}
{"cmd": "bench", "queries": 200, "duration": 5, "request_id": "..."}
```

### Bundle shape (excerpt)

```json
{
  "symbol": {
    "symbol_id": "a1b2…",
    "path": "crate::util::State",
    "kind": "struct",
    "signature": "…",
    "docs": "…"
  },
  "children": [
    /* fields/methods/variants, ordered */
  ],
  "extern_refs": [
    /* extern crate types referenced, unless in .ctignore */
  ],
  "impl_ranges": [
    {
      "file": "src/util/state.rs",
      "file_digest": "blake3:…",
      "line_start": 42,
      "line_end": 118
    },
    {
      "file": "src/util/state_ext.rs",
      "file_digest": "blake3:…",
      "line_start": 10,
      "line_end": 57
    }
  ],
  "order": "bfs",
  "invariants": {
    "range_1_based_inclusive": true
  }
}
```

---

## 10) Performance Targets & Benchmarking

### Targets (warm daemon)

* `find` name: P50 1–10 ms, P99 < 20 ms
* `doc` path: P50 1–5 ms
* `ls` path (shallow): P50 3–15 ms
* `export` bundle (to cap): P50 5–50 ms, P99 < 120 ms
* Memory: O(#symbols) for maps; typical 50–250 MB large workspaces

### Benchmarking command (`ct bench`)

* Measures:

  * **Initial index time** (cold DB) and resulting counts
  * **Incremental index time** after touching N files (via watcher simulation)
  * **Query latency** distributions for mixed `find/doc/ls/export`
* Output: human summary + optional JSON when `--json` is set.

---

## 11) UX Details

* REPL prompt: `(ct crate::engine::util)>` (reflects cwd).
* Expansion operators: `>` children, `<` parents; stacking allowed; **no soft cap**.
* Docs toggle: `-d/--docs`.
* Visibility filters: `--vis public|private|all` (aliases: `--public`, `--private`).
* Status filters: `-u/--unimplemented`, `-t/--todo`.
* Pretty output: `--pretty`; default compact JSON for agents.
* `--help` on every binary and `ct help <command>` for subcommands.

---

## 12) Security & Safeguards

* Operates **read‑only** within `workspace_allow`.
* Canonicalize and resolve symlinks; reject paths escaping the workspace.
* Daemon binds to **local IPC** only; optional TCP fallback is explicit opt‑in and binds to `127.0.0.1` with a per‑session token and idle timeouts.
* Redact file contents in logs; optional path redaction.

---

## 13) Indexing & Reindexing

* **MVP backend**: `rustdoc --output-format json` ingestion with **multi-crate workspace support**.
* **Multi-crate handling**: 
  * Run `rustdoc --output-format json` for each crate in the workspace (detected via `cargo metadata`)
  * Merge all rustdoc JSON outputs into a unified symbol database
  * Maintain crate boundaries in the database for proper path resolution
  * Handle cross-crate dependencies and re-exports
* **Incremental**: file watcher detects changes; re‑ingest only affected crates/files; update rows and auxiliary maps; perform **atomic DB swap**.
* Fingerprint includes `rustc_hash`, features, target, and `--print cfg` snapshot. Mismatch triggers full reindex.

---

## 14) Testing & Acceptance

### Test assets

* A **multi-crate test workspace** (under `tests/fixtures/mini_workspace/`) containing:
  * Multiple interdependent crates with cross-crate imports
  * Features modules, impls, traits, generics, `unimplemented!()`, `todo!()`, and `TODO/FIXME` comments
  * Re-exports and pub use statements across crate boundaries

### Unit & integration

* Property tests: ranking stability; path normalization; expansion invariants.
* Integration tests: daemon IPC; atomic reindex; watcher‑triggered incremental updates.
* Load test: 10 concurrent clients @ 100 rps mixed queries; assert P99 SLAs and no unbounded memory growth.

### Acceptance criteria

* `ct-daemon --idx .` builds DB for entire workspace; `ct status` reports implemented/unimplemented/todo counts across all crates.
* `ct find State` returns ranked matches from all workspace crates; respects crate boundaries in paths.
* `ct doc crate_a::util::State` and `ct doc crate_b::types::Config` work across different workspace crates.
* `ct ls crate_a::util >` returns children deterministically, including cross-crate re-exports.
* `ct export --bundle crate::util::State -d` returns a bundle with `impl_ranges` and file digests.
* REPL: `cd`, `ls`, `doc`, `find`, `export`, `quit`; tab completion works; can navigate between crates.
* Hard cap enforcement returns **decision prompt** when exceeded.
* `.ctignore` prevents deep expansion of ignored crates and modules.
* `ct bench` prints initial and incremental index metrics and query latencies.

---

## 15) Milestones

* **M0 – Skeleton (1–2 wks)**: Daemon scaffolding, IPC, config, normalized SQLite schema, JSON protocol & envelopes, `--help`.
* **M1 – Index & Query (2–3 wks)**: rustdoc JSON ingestion, in‑memory maps, deterministic ordering, `find/doc/ls`, REPL.
* **M2 – Export & Expansion (1–2 wks)**: `export --bundle`, `>`/`<` stacking, hard‑cap decision prompt, `.ctignore`, visibility & status filters.
* **M3 – Watch & Bench (1–2 wks)**: file watching + incremental reindex; `ct bench`; autostart polish.
* **M4 – Hardening**: perf profiling, large‑repo tests, cross‑platform sockets/pipes, crash recovery.
* **Phase 2 (post‑MVP)**: HIR backend, implementors/who‑uses/call graph, editor `open`, MCP server.

---

## 16) Risks & Mitigations (selected)

* **Re‑exports & aliasing**: risk of duplicate parents ⇒ track re‑exports separately (`reexports(symbol_id, parent_id, alias)` in a future migration); normalize parent as defining module.
* **Watcher churn** on large monorepos ⇒ batch/coalesce events, backoff strategy; cap parallel ingestion.
* **JSONL fragility** ⇒ escape newlines in strings; consider length‑prefix framing in v2 if needed.

---

## 17) Configuration (`ct.toml` excerpt)

```toml
# Transport & daemon
transport = "auto"          # auto|unix|pipe|tcp (tcp disabled unless explicitly set)
autostart = true
socket_path = "/tmp/ctd.sock"
pipe_name = "\\\\.\\pipe\\ctd"
tcp_addr = "127.0.0.1:48732"
allow_full_context = false   # controls whether "full" decision option is offered

# Workspace
workspace_allow = ["/abs/path/to/ws"]

# Slicing
max_context_size = 16000     # hard cap (characters)

# Index DB
db_dir = "${XDG_CACHE_HOME}/ct/${WORKSPACE_FINGERPRINT}"
db_file = "symbols.sqlite"

# Bench
bench_queries = 200
bench_duration_s = 5
```