# ct - Context Tool

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

A CLI tool for getting minimal context in Rust crates, designed with AI agents as first-class citizens.

## Overview

`ct` (crate tool) provides symbol-centric code exploration with deterministic, minimal slices of code. It solves the problem where agents and humans waste tokens/time skimming entire files to understand or modify a small set of symbols.

The tool indexes Rust workspaces into a persistent symbol database and answers queries with tight, deterministic slices focused on the symbols you care about.

## Features

- **Symbol-centric queries** - Search and explore code by symbol names, not files
- **Progressive expansion** - Start with minimal context and expand as needed
- **Multi-crate workspaces** - Full support for complex Rust workspaces
- **Fast queries** - Hot in-memory indices for sub-20ms response times
- **File watching** - Automatic incremental updates as code changes
- **Deterministic output** - Same query always returns same results
- **AI-friendly** - Designed for token efficiency and clarity

## Installation Steps

1. Build all binaries:
`cargo build --release --all`

2. Install binaries to your PATH (choose one option):

### Option A

run the bash script

```bash
./build_and_copy.sh
```

### Option B - System-wide installation

Copy to `/usr/local/bin` (requires sudo)

```bash
sudo cp target/release/ct /usr/local/bin/
sudo cp target/release/ct-daemon /usr/local/bin/
sudo cp target/release/ctrepl /usr/local/bin/
```

### Option C - User installation

Copy to `~/.local/bin` (create if needed)

```bash
mkdir -p ~/.local/bin
cp target/release/ct ~/.local/bin/
cp target/release/ct-daemon ~/.local/bin/
cp target/release/ctrepl ~/.local/bin/
```

### Option D - root

Copy the binary files to the root of the project

### Start the daemon

```bash
# Start the daemon (auto-cleans if needed)
ct daemon start

# Or start with explicit cache cleaning
ct daemon start --clean

# Check daemon status
ct daemon status

# Restart daemon (automatically cleans cache)
ct daemon restart

# Stop daemon
ct daemon stop
```

### Basic commands

```bash
# Find symbols by name
ct find MyStruct

# Show documentation for a symbol
ct doc crate::util::State

# List symbols with expansion
ct ls crate::util::State >  # Show children (fields, methods)
ct ls crate::util::State <  # Show parent context

# Export symbol bundles
ct export crate::util::State crate::api::Handler

# Check implementation status
ct status --unimplemented
```

### Interactive REPL

```bash
# Start the interactive REPL with tab completion
ctrepl
```

## TODO

1) remake the ct-indexer crate using code from plan-gen

## Query Syntax

### Symbol paths

- Canonical paths: `crate::module::Type`
- Cross-crate: `other_crate::module::Type`

### Expansion operators

- `>` - Expand children (fields, methods, variants)
- `<` - Expand parents (context above definition)
- Multiple operators can be chained: `crate::Type >>`

### Filters

- `--visibility` - Filter by visibility: `public`, `private`, `all`
- `--status` - Filter by implementation: `implemented`, `unimplemented`, `todo`

## Configuration

Create a `ct.toml` file in your project root to customize behavior:

```toml
# Auto-clean cache on daemon start
auto_clean_on_start = true

# Cache time-to-live in hours
cache_ttl_hours = 24

# Maximum context size
max_context_size = 10000

# Other options...
```

## Architecture

The project consists of three main components:

1. **ct-daemon** - Background indexing service
   - Indexes workspaces using `rustdoc --output-format json`
   - Maintains SQLite database with BLAKE3 symbol IDs
   - Watches files for incremental updates
   - Serves IPC requests via JSONL protocol

2. **ct** - CLI tool for one-shot commands
   - Communicates with daemon via IPC
   - Provides subcommands for common queries
   - Handles output formatting and character limits

3. **ctrepl** - Interactive REPL
   - Tab completion for symbol navigation
   - Stateful exploration of symbol trees
   - Human-friendly interface

## Performance

Target performance for large workspaces:

| Operation | P50 | P99 |
|-----------|-----|-----|
| Find | 1-10ms | <20ms |
| Doc | 1-5ms | <10ms |
| Export | 5-50ms | <120ms |

Memory usage: 50-250MB for large workspaces

## Development

```bash
# Run tests
cargo test

# Run benchmarks
cargo bench

# Check formatting
cargo fmt -- --check

# Run linter
cargo clippy
```

## Project Status

This is an MVP (Minimum Viable Product) focused on read-only operations. Future versions may include:

- Rename operations
- Write/refactoring capabilities
- Additional language support
- LSP integration

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is dual-licensed under either:

- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)

at your option.

## Acknowledgments

Designed specifically for AI agents and developers who need efficient, focused code context without the noise.
