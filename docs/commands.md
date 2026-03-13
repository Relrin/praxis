# praxis Commands

## praxis build

Build a context bundle from a repository.

### Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--task` | string | required | Task description for relevance scoring |
| `--repo` | path | `.` | Path to the repository root |
| `--token-budget` | integer | `8000` | Total token budget |
| `--buffer-pct` | float | `0.10` | Soft buffer percentage (ignored in strict mode) |
| `--strict` | bool | `false` | Hard cap at --token-budget with no buffer |
| `--output` | path | `context.json` | Output file path |
| `--format` | enum | `json` | Output format: `json`, `markdown`, or `both` |
| `--max-file-size` | integer | `204800` | Maximum file size in bytes to include |
| `--stdout` | bool | `false` | Write output to stdout instead of a file |
| `--conversation` | path | none | Path to a conversation file for memory extraction |
| `--vector` | bool | `false` | Enable vector-enhanced scoring (requires `vector` feature) |
| `--vector-weight` | float | `0.30` | Weight for vector similarity in hybrid score (0.0-1.0) |

### New in Phase 3: --vector

When `--vector` is provided, praxis runs incremental vector indexing and blends semantic similarity with deterministic scores. See [vector-indexing.md](vector-indexing.md) for details.

### New in Phase 2: --conversation

When `--conversation` is provided, praxis:
1. Extracts structured memory (constraints, decisions, open questions, stage markers)
2. Boosts file relevance scores based on stage marker mentions
3. Re-sorts files by boosted scores
4. Truncates memory to fit the memory budget bucket
5. Includes `conversation_memory` in the output bundle

### Examples

```bash
# Basic build
praxis build --task "Fix the authentication bug" --repo ./my-project

# Build with conversation context
praxis build --task "Implement OAuth2" --repo ./api --conversation chat.md

# Strict mode with stdout output
praxis build --task "Refactor parser" --strict --stdout --format json
```

---

## praxis summarize

Extract structured memory from a conversation file.

### Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--input` | path | required | Path to the primary conversation file |
| `--mode` | enum | `flat` | Rendering mode: `flat`, `hierarchical`, `decision-focused` |
| `--ignore-line-comments` | bool | `false` | Skip lines starting with comment markers |
| `--since` | integer | none | Only include items from this turn index onward |
| `--merge` | path(s) | none | Additional conversation files to merge |
| `--output` | path | stdout | Output file path |
| `--format` | enum | `json` | Output format: `json` or `markdown` |

### Modes

- **flat**: All items in a single list, sorted by turn index
- **hierarchical**: Items grouped by classification (constraints, decisions, questions)
- **decision-focused**: Decisions with their resolved questions paired together

### Examples

```bash
# Extract to stdout as JSON
praxis summarize --input conversation.md --mode flat --format json

# Hierarchical markdown output to file
praxis summarize --input chat.md --mode hierarchical --format markdown --output summary.md

# Filter to recent turns only
praxis summarize --input chat.md --since 10

# Merge multiple conversation files
praxis summarize --input part1.md --merge part2.md part3.md --format json
```

---

## praxis diff

Compute file and symbol-level changes between two git refs.

### Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--from` | string | `main` | Git ref for the base (older) version |
| `--to` | string | `HEAD` | Git ref for the target (newer) version |
| `--repo` | path | `.` | Path to the git repository root |
| `--output` | path | `diff.json` | Output file path |
| `--format` | enum | `json` | Output format: `json` or `markdown` |
| `--token-budget` | integer | none | Token budget for pruning diff context |
| `--strict` | bool | `false` | Hard cap at --token-budget |
| `--conversation` | path | none | Conversation file for cross-referencing stage markers |

### Examples

```bash
# Diff between main and current branch
praxis diff --repo ./my-project

# Diff specific refs
praxis diff --from v1.0 --to v1.1 --repo ./api

# Diff with conversation cross-reference
praxis diff --from main --to feature/auth --conversation chat.md

# Diff with budget pruning
praxis diff --token-budget 4000 --strict
```

---

## praxis inspect

Inspect an existing bundle with a human-readable audit.

### Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| (positional) | path | required | Path to the bundle file (context.json or diff.json) |
| `--verbose` | bool | `false` | Include dropped/skipped file list |
| `--json` | bool | `false` | Output the audit as structured JSON |

### Examples

```bash
# Inspect a context bundle
praxis inspect context.json

# Inspect a diff bundle with verbose output
praxis inspect diff.json --verbose

# Get structured JSON audit
praxis inspect context.json --json
```

---

## praxis prune

Re-run budget allocation on an existing bundle with a new token budget.

### Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| (positional) | path | required | Path to an existing context bundle |
| `--token-budget` | integer | required | New token budget to prune to |
| `--strict` | bool | `false` | Hard cap at --token-budget |
| `--preserve-files` | string(s) | none | Comma-separated file paths to always keep at full inclusion |
| `--output` | path | `context_pruned.json` | Output file path |
| `--format` | enum | `json` | Output format: `json`, `markdown`, or `both` |

### Examples

```bash
# Prune to half budget
praxis prune context.json --token-budget 4000

# Prune with preserved files
praxis prune context.json --token-budget 2000 --preserve-files src/auth.rs,src/main.rs

# Strict mode with custom output
praxis prune context.json --token-budget 3000 --strict --output small.json
```

---

## praxis index

Build or update the vector index for a repository. Requires the `vector` feature.

### Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--repo` | path | `.` | Path to the repository root |
| `--max-file-size` | integer | `204800` | Maximum file size in bytes to include |
| `--force` | bool | `false` | Drop and rebuild the entire vector index from scratch |

### Examples

```bash
# Incremental index (only changed files)
praxis index --repo ./my-project

# Force full re-index
praxis index --repo ./my-project --force

# Build with vector-enhanced scoring
praxis build --task "fix auth" --repo ./my-project --vector

# Custom vector weight
praxis build --task "fix auth" --vector --vector-weight 0.5
```

See [vector-indexing.md](vector-indexing.md) for full documentation.
