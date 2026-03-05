# Todo MCP Server

A Rust MCP (Model Context Protocol) server that exposes **todo lists** as resources and four tools: **add-todo**, **complete-todo**, **pick-todo**, and **wait-for-update**. Each folder can have multiple named lists; every command accepts a **list name** to operate on. All operations are thread-safe and atomic. Lists are stored as human-readable markdown files in the directory where the server was started.

## Requirements

- **Rust 1.85+** (for edition 2024). Run `rustup update stable` if needed.

## Build

```bash
cargo build --release
```

## Quality gate (CI / contributors)

The project is kept warning-free. Build and tests use `-D warnings` (see `.cargo/config.toml`). To run the full quality gate locally (build + test + clippy):

```bash
./scripts/quality.sh
```

Or manually: `cargo build --all-targets && cargo test --all-targets && cargo clippy --all-targets -- -D warnings`, then `cargo build --release && cargo clippy --release -- -D warnings`. Use this before pushing or in CI to ensure no new warnings. The release-profile check catches profile-specific drift (e.g. different code paths or lints under opt-level).

### Contributing: warning policy and local workflow

**Policy:** New warnings are not allowed. The codebase is kept warning-free; CI runs the quality gate on every push and pull request and will fail if any rustc or Clippy warning is introduced.

**Local workflow:**

1. Before committing or opening a PR, run the quality gate:
   ```bash
   ./scripts/quality.sh
   ```
2. If you see **unused** or **dead_code** warnings: remove the unused item, or add `#[allow(dead_code)]` only when the code is intentionally kept (e.g. used via a trait or for future use), with a short comment.
3. If you see **Clippy** suggestions: apply the suggested fix (e.g. use `strip_prefix` instead of manual indexing, collapse nested `if`s, use `std::io::Error::other`). Run `cargo clippy -- -D warnings` to see all lints.
4. The quality script uses `--all-targets` so the binary, tests (unit + integration), and Clippy are all checked. Fix any failure before pushing.

**Clippy baseline:** We use Clippy with `-D warnings` (deny all). No lints are allowed or suppressed except where documented (e.g. `#[allow(dead_code)]` with a comment). New lints added in future Clippy/Rust versions will fail CI; fix them or add a targeted `#[allow(clippy::lint_name)]` with a brief justification so the policy stays consistent.

**Optional pre-commit hook:** To run the quality gate before each commit, install the script: `cp scripts/pre-commit.sh .git/hooks/pre-commit && chmod +x .git/hooks/pre-commit`.

**Checklist (contributors and reviewers):**
- [ ] Run `./scripts/quality.sh` before pushing; it must pass with zero warnings.
- [ ] Do not introduce new `#[allow(...)]` without a one-line comment justifying it.
- [ ] In review: confirm CI passed and no warning suppressions were added without justification.

## Run

From the directory you want to use as the todo list root (one `.md` file per named list, e.g. `work.md`, `personal.md`):

```bash
./target/release/todo-mcp
```

Or with cargo:

```bash
cd /path/to/your/workspace
cargo run --release
```

The server uses **Streamable HTTP (SSE)** transport. It listens on **port 8080** by default. Set `MCP_PORT` to use another port (e.g. `MCP_PORT=3000 ./target/release/todo-mcp`).

- **Endpoint:** Use `http://localhost:8080/mcp` in your MCP client (root `http://localhost:8080` also works).

## CLI add command

With the server already running, you can add a todo from the shell. The **server** performs list creation (if the list does not exist), id generation, and notifications; the CLI only calls the MCP server.

### Quick-start

**Terminal user:** Start the server in one terminal, then in another run `./target/release/todo-mcp add <list> "Title"` (or use `-i` to be prompted for list/title/body). Use `--summary` for a one-line summary plus id; use `--dry-run` to preview without sending.

**Script/automation:** Use `--timeout N` to avoid hangs, `-j`/`--json` for machine-readable output, and non-interactive args. Example: `./target/release/todo-mcp add -t 5 -j work "Item" ""` (exit 0 and parse JSON, or check exit code on failure). Set `NO_COLOR=1` for plain output in logs.

```bash
# Default server (http://127.0.0.1:8080); prints the new todo id
./target/release/todo-mcp add mylist "Fix login" "Optional body text"

# Custom port via env (same as server MCP_PORT)
MCP_PORT=3000 ./target/release/todo-mcp add work "Review PR" ""

# Explicit --port or --server
./target/release/todo-mcp add --port 3000 personal "Buy milk"
./target/release/todo-mcp add --server http://192.168.1.10:8080 shared "Team task"

# Body from stdin (use - as body argument)
echo "Long description here" | ./target/release/todo-mcp add mylist "Title" -

# JSON output for scripting (id, list, title, server)
./target/release/todo-mcp add mylist "Title" "" --json

# Pretty-print JSON; non-interactive timeout (e.g. in scripts)
./target/release/todo-mcp add mylist "Title" "" --json --pretty --timeout 5
```

Use `--help` or `add --help` for full usage, options, and exit codes.

Exit codes: 1 = connection failure (or timeout), 2 = invalid usage/input, 3 = MCP/server error (e.g. invalid list name, title too long).

**Troubleshooting:** Server not running → exit 1 and "connection" or "failed to connect" (start the server first in another terminal). Invalid list name (e.g. spaces/special chars) → exit 3 and server error message; use only letters, numbers, underscores, hyphens. Title empty or too long → exit 3. Timeout → use `--timeout N` or ensure server is reachable and MCP_PORT (or --port) matches the server.

### Copy-paste snippets

**Shell script (capture id):**
```bash
ID=$(./target/release/todo-mcp add work "Deploy fix" "" --timeout 10) || exit 1
echo "Created todo #$ID"
```

**CI / non-interactive (fail fast, no hang):**
```bash
./target/release/todo-mcp add --port "${MCP_PORT:-8080}" --timeout 5 ci-list "Build $CI_JOB_ID" "" || exit 1
```

**Multiline description from file or heredoc:**
```bash
./target/release/todo-mcp add docs "Doc task" - <<'EOF'
Line one.
Line two.
EOF
```

**Custom endpoint (remote server):**
```bash
./target/release/todo-mcp add --server http://myhost:8080 shared "Team task" "" --timeout 15
```

### MCP vs CLI: when to use which

| Use case | Prefer | Why |
|----------|--------|-----|
| Add todo from shell / script | **CLI** `add` | One command, no MCP session; supports `--timeout`, `--json`, stdin body. |
| Add todo from Cursor/IDE or another MCP client | **MCP** `add-todo` | Same server; tools and resources stay in one session; you get list resources and wait-for-update in the same client. |
| Automate in CI (e.g. create todo per job) | **CLI** | Non-interactive, exit codes, `--timeout` to avoid hangs. |
| Watch list changes or read list content in IDE | **MCP** | Use `todo://list/{name}` resource and `wait-for-update` in the same session. |
| One-off add from terminal with server already running | **CLI** | Quick: `todo-mcp add mylist "Title"`. |

**Side effects:** Both paths hit the same server and store. Adding via CLI or MCP creates/updates the same list file and triggers the same `wait-for-update` wakeups for any connected MCP clients.

## MCP API

### Resources

- **`todo://list/{name}`** – One resource per todo list. `list_resources` returns all existing lists (all `*.md` files in the folder). Reading a resource returns that list as markdown: each item has a numeric **id** (e.g. `- [ ] #1 Title` / `- [x] #2 Done`). Use the id with **complete-todo**. List names use only letters, numbers, underscores, and hyphens (e.g. `work`, `personal`, `todo-list`).

### Tools

Every tool takes a **list_name** to choose which list to use.

1. **add-todo**  
   - **Arguments:** `list_name` (string), `title` (string), `body` (string)  
   - Adds a new pending todo to the named list. Creates the list file if it doesn’t exist. Returns the new todo’s **id** (numeric, sequential per list). Use this id with **complete-todo**.

2. **complete-todo**  
   - **Arguments:** `list_name` (string), `id` (number)  
   - Marks the todo with the given id as complete. The id is returned by **add-todo** or shown in the list resource / **pick-todo** output.

3. **pick-todo**  
   - **Arguments:** `list_name` (string)  
   - Returns a random pending todo (title and body) from the named list, or a message if there are none.

4. **wait-for-update**  
   - **Arguments:** `list_name` (string)  
   - Blocks until the named list is updated, then returns a message. This is a long-polling tool and does not return until the list changes.

## Storage

- **One file per list** in the **current working directory**: `{list_name}.md` (e.g. `work.md`, `personal.md`). List names must contain only letters, numbers, underscores, and hyphens, and be at most 64 characters.
- Format: each item has a title line and an optional body (indented with 2 spaces):
  - Pending: `- [ ] Title` with optional body on following lines, indented.
  - Done: `- [x] Title` with optional body on following lines, indented.
- Example `work.md`:
  ```markdown
  - [ ] Fix login timeout
    Users on slow networks hit a 5s timeout; we should make it configurable and document the env var.
  - [x] Add rate limiting
    Implemented per-IP limits in the middleware.
  ```
- All reads and writes are done under a single mutex so each tool call sees a consistent, atomic view.
- List files are locked with shared (read) or exclusive (write) locks (`fs2`) so multiple server processes using the same directory do not corrupt data; one process blocks until another releases the lock.

## Example client config (e.g. Cursor, Claude Desktop)

With the server running (e.g. `./target/release/todo-mcp` in the folder that holds your todo lists):

```json
{
  "mcpServers": {
    "todo": {
      "url": "http://localhost:8080/mcp"
    }
  }
}
```

Use a different port if you set `MCP_PORT` (e.g. `"url": "http://localhost:3000/mcp"`).

## Configuration

Environment variables and limits:

| Variable | Default | Description |
|----------|---------|-------------|
| `MCP_BASE_DIR` | current directory | Directory for `{list_name}.md` list files. Created if missing. |
| `MCP_HOST` | `0.0.0.0` | Bind address. |
| `MCP_PORT` | `8080` | Port for HTTP and MCP. |
| `MCP_WAIT_POLL_MS` | `500` | Poll interval (ms) for `wait-for-update` when watching for file changes. |
| `MCP_BODY_LIMIT_BYTES` | `2097152` (2 MiB) | Max request body size; larger requests get 413. |

Limits: list name ≤ 64 characters; title ≤ 200 characters; body ≤ 4096 characters (see tool validation).
