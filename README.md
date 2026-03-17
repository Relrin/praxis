# Praxis
Context bundling for AI-assisted development

## Why was it implemented?
The idea of implementing such tool came to me after exploring LLM capabilities and interacting with different models. As the part of the workflow, I'm trying to keep the context as small as possible, while providing enough information from the beginning of the session when starting a new conversation.

Considering that developers need to manage complex codebases, guiding an AI by providing relevant information would make my work a bit more efficient (and enjoyable!). By the other side, I'm also interested in reducing the amount scans, tool calls, in order to reduce the token consumption when working with online or self-hosted AI models.

## What it does
The CLI tool does few things:
1. **Scans** the given repository (respecting `.gitignore` and `.praxisignore`)
2. **Scores** each file for relevance to the task description by keyword matching, symbol analysis, git history, and dependency signals
3. **Allocates** a token budget across files: high-relevance files get full content, medium-relevance files get signatures only, low-relevance files are summarized or skipped
4. **Outputs** a structured context (JSON, Markdown, or both) that can be feed into an LLM

## Features
- **Multi-signal relevance scoring** - combines keyword overlap, symbol matching, git recency, and dependency analysis to rank files
- **Token budget management** - soft mode (with safety buffer) or strict mode (hard cap), with automatic allocation across task, code, memory, and metadata
- **7 language analyzers** - Rust, Python, TypeScript/JavaScript, Go, C++, Elixir, AngelScript. For each supported language the tool extracts functions, classes, structs, traits, and other symbols
- **Conversation memory** - extract structured context (constraints, decisions, open questions) from chat logs and fold it into the bundle
- **Git diff analysis** - compute file-level and symbol-level changes between any two git refs
- **Vector-enhanced search** (optional) - semantic similarity via local embeddings (LanceDB + MiniLM), blended with deterministic scores
- **Multiple output formats** - JSON for tooling, Markdown for humans, or both
- **Custom ignore rules** - `.praxisignore` files work like `.gitignore` for additional exclusions
- **Configurable** - The tooling does allow to tweak the behavior, by overriding config located by the `.praxis/config.toml` path

## Installation

### Regular process
1. Download binary file in according to the used operating system from the [releases page](https://github.com/Relrin/praxis/releases).
2. Link executable/binary file to operating system, so you could invoke `praxis` from any place:

    - Linux / Mac OS

      Move the binary file to the `/usr/local/bin` directory and restart the terminal. For example, it could be like this:
        ```
        mv ~/Downloads/praxis /usr/local/bin
        ```

    - Windows

        1. Right-click on the Windows Logo and select the `System` menu item.
        2. Click on the `Advanced System Settings` button.
        3. Click on the `Environment Variables` button.
        4. Select your `PATH` variable and click in the `Edit` button.
        5. Click on the `New` button.
        6. Add the file path to the directory with the `praxis` executable.
        7. Click on the `OK` button a couple of times for applying changes.

### From sources

Build from source (requires Rust 2024 edition):

```bash
git clone https://github.com/Relrin/praxis.git
cd praxis
cargo build --release
```

With vector search support:

```bash
cargo build --release --features vector
```

The binary will be at `target/release/praxis`.

## Commands

### `praxis build`

**When to use:** You have a task to work on and need a focused snapshot of the relevant code from a repository.

```bash
# Scan the current repo and build a context bundle for a specific task
praxis build --task "Fix the authentication middleware timeout bug"

# Target a different repo, output as Markdown, with a larger budget
praxis build --task "Add pagination to the users endpoint" \
  --repo ./backend --format markdown --token-budget 12000

# Include conversation context from a previous chat session
praxis build --task "Implement OAuth2 flow" \
  --conversation chat.md --format both

# Strict budget cap, pipe directly to another tool
praxis build --task "Refactor the parser" --strict --stdout --format json
```

### `praxis summarize`

**When to use:** You have a conversation log (e.g., from a design discussion or debugging session) and want to extract the key decisions, constraints, and open questions as structured data.

```bash
# Extract memory from a conversation file
praxis summarize --input conversation.md --mode flat --format json

# Group by category (constraints, decisions, questions)
praxis summarize --input chat.md --mode hierarchical --format markdown --output summary.md

# Only recent turns, merging multiple files
praxis summarize --input part1.md --merge part2.md part3.md --since 10
```

### `praxis diff`

**When to use:** You want to understand what changed between two git refs — not just which files, but which symbols (functions, structs, etc.) were added, modified, or removed.

```bash
# Diff between main and the current branch
praxis diff --repo ./my-project

# Diff specific tags
praxis diff --from v1.0 --to v1.1

# Cross-reference changes with a conversation log
praxis diff --from main --to feature/auth --conversation chat.md

# Budget-constrained diff output
praxis diff --token-budget 4000 --strict
```

### `praxis inspect`

**When to use:** You already have a bundle (from `build` or `diff`) and want to audit it — see what was included, what was skipped, and how the token budget was spent.

```bash
# Quick audit of a context bundle
praxis inspect context.json

# Verbose output including skipped files
praxis inspect context.json --verbose

# Machine-readable audit
praxis inspect diff.json --json
```

### `praxis prune`

**When to use:** You have an existing bundle but need to shrink it to a smaller token budget — without re-scanning the repository.

```bash
# Shrink a bundle to 4000 tokens
praxis prune context.json --token-budget 4000

# Keep specific files at full inclusion regardless of budget pressure
praxis prune context.json --token-budget 2000 --preserve-files src/auth.rs,src/main.rs

# Strict cap with custom output path
praxis prune context.json --token-budget 3000 --strict --output small.json
```

### `praxis index`

**When to use:** You want to enable vector-based semantic search for more accurate relevance scoring. Run this once (or after significant code changes), then use `--vector` with `build`.

Requires the `vector` feature at compile time.

```bash
# Build or update the vector index (incremental — only changed files)
praxis index --repo ./my-project

# Force a full rebuild
praxis index --repo ./my-project --force

# Then use it during build
praxis build --task "fix auth" --repo ./my-project --vector --vector-weight 0.5
```

## How scoring works

Each file receives a relevance score from 0 to 1, computed as a weighted combination of four signals:

| Signal | Weight | What it measures |
|--------|--------|------------------|
| Keyword overlap | 40% | How many task words appear in the file content |
| Symbol overlap | 30% | How many task words match extracted symbol names |
| Git recency | 20% | How recently the file was modified in git history |
| Dependency match | 10% | Whether the file's project dependencies are mentioned in the task |

When vector search is enabled, the deterministic score is blended with semantic similarity (default 70/30 split).

For full CLI reference, see [`docs/commands.md`](docs/commands.md). For vector search configuration, see [`docs/vector-indexing.md`](docs/vector-indexing.md).

## License

The praxis project is published under BSD 3-clause license. For more details read the [LICENSE](https://github.com/Relrin/praxis/blob/master/LICENSE) file.
