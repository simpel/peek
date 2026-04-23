# peek — Inline Shell Autocomplete Daemon

## Overview

peek is a background daemon that provides inline autocomplete suggestions for package manager scripts, Makefile targets, docker-compose services, and other common CLI tools. It renders a floating dropdown below the cursor as you type, powered by fuzzy matching and frecency-based sorting.

**Target:** macOS, any terminal emulator, zsh + bash + fish.
**Language:** Rust.
**Distribution:** Homebrew tap with `brew services` support.

---

## Architecture

```
┌─────────────┐       Unix socket        ┌──────────────┐
│ Shell Plugin │ ◄──────────────────────► │  peek daemon  │
│ (zsh/bash/   │   JSON request/response  │              │
│  fish hooks) │                          │  - scanner   │
└─────────────┘                          │  - cache     │
       │                                  │  - frecency  │
       ▼                                  │  - fs watcher│
  Terminal UI                             └──────────────┘
  (ANSI rendering                               │
   below cursor)                          launchd agent
                                          (brew services)
```

### Components

1. **peek daemon** (`peekd`) — long-running background process
2. **Shell plugins** — zsh ZLE widget, bash readline hook, fish keybinding
3. **CLI** (`peek`) — manage the daemon, configure settings, query manually

---

## Daemon (`peekd`)

### Lifecycle

- **Primary:** launchd agent installed via `brew services start peek`. Starts on login, restarts on crash.
- **Fallback:** Shell plugin checks if daemon is running on init. If not, spawns it in the background.
- **Socket:** `~/.peek/peek.sock` (Unix domain socket).
- **PID file:** `~/.peek/peekd.pid`.
- **Logs:** `~/.peek/logs/peekd.log` (rotation at 10MB).

### Directory Scanner

On receiving a directory context (triggered by shell `cd` hook or query):

1. Check in-memory cache for the directory.
2. If miss or stale, scan for known files:
   - `package.json` → extract `scripts` object + detect lockfile to determine package manager
   - `Makefile` / `GNUmakefile` / `makefile` → extract phony/non-file targets
   - `docker-compose.yml` / `docker-compose.yaml` / `compose.yml` → extract service names
   - `Cargo.toml` → detect Rust project, suggest standard cargo subcommands
3. Cache result keyed by directory path + file mtime.

### File Watcher

- Uses `kqueue` (macOS) via the `notify` crate.
- Watches known config files in the current directory.
- On change, invalidates cache entry and re-scans.
- Only watches directories that have been visited (not recursive global watch).

### Frecency Engine

Tracks command usage per directory:

- **Storage:** `~/.peek/history.db` (SQLite via `rusqlite`).
- **Schema:** `(directory TEXT, command TEXT, tool TEXT, timestamp INTEGER, count INTEGER)`.
- **Scoring:** `score = frequency_weight * count + recency_weight * decay(now - last_used)`. Decay is exponential with a half-life of 7 days.
- **Sorting:** Suggestions sorted by frecency score descending. Unscored items appear after scored ones in file order.

---

## Shell Plugins

### Communication Protocol

Request (shell → daemon, JSON over Unix socket):

```json
{
  "type": "suggest",
  "cwd": "/Users/joel/project",
  "line": "pnpm ",
  "cursor": 5
}
```

Response (daemon → shell, JSON):

```json
{
  "suggestions": [
    {"name": "dev", "preview": "next dev --turbo", "score": 0.95},
    {"name": "build", "preview": "next build", "score": 0.80},
    {"name": "test", "preview": "vitest", "score": 0.60}
  ],
  "tool": "pnpm"
}
```

Directory change notification (shell → daemon):

```json
{
  "type": "cd",
  "cwd": "/Users/joel/other-project"
}
```

Command execution tracking (shell → daemon):

```json
{
  "type": "executed",
  "cwd": "/Users/joel/project",
  "command": "dev",
  "tool": "pnpm"
}
```

### zsh Integration

- Hook into ZLE with a custom widget bound to the `self-insert` and `accept-line` widgets.
- On each keypress after a recognized tool name + space, query the daemon.
- Render dropdown using ANSI escape codes below the current line.
- Arrow keys navigate, Enter accepts, Escape dismisses.
- `chpwd` hook sends `cd` notification.
- `preexec` hook sends `executed` notification for tracked commands.

### bash Integration

- Use `PROMPT_COMMAND` or `DEBUG` trap for cd tracking.
- `bind -x` for keypress interception (limited compared to ZLE).
- Readline's `rl_bind_keyseq` for completion trigger.
- Same ANSI rendering as zsh.

### fish Integration

- `fish_postexec` and `fish_prompt` events for tracking.
- `bind` command for keypress hooks.
- Fish has native event handling that simplifies integration.

### Setup

User adds to their shell rc file:

```sh
# zsh (~/.zshrc)
eval "$(peek init zsh)"

# bash (~/.bashrc)
eval "$(peek init bash)"

# fish (~/.config/fish/config.fish)
peek init fish | source
```

---

## Inline Dropdown Rendering

### Display

- Rendered below the cursor using ANSI escape sequences.
- Max 8 items visible, scrollable if more.
- Each item: `name` left-aligned, `preview` right-aligned and dimmed.
- Selected item highlighted with inverse colors.
- Fuzzy match characters highlighted in the suggestion text.

```
$ pnpm d
┌──────────────────────────────────┐
│ ► dev          next dev --turbo  │
│   dev:debug    next dev --debug  │
│   docker       docker compose up │
└──────────────────────────────────┘
```

### Keybindings

| Key        | Action                            |
|------------|-----------------------------------|
| ↑ / ↓     | Navigate suggestions              |
| Tab        | Accept selected suggestion        |
| Enter      | Accept and execute                |
| Escape     | Dismiss dropdown                  |
| Any char   | Filter suggestions (fuzzy match)  |
| Backspace  | Update filter / dismiss if empty  |

### Rendering Strategy

- Save cursor position, move below prompt, draw box, restore cursor.
- On dismiss/accept, clear the rendered area.
- Handle terminal resize (SIGWINCH) to reposition.
- If near bottom of terminal, render above the cursor instead.

---

## Fuzzy Matching

- Algorithm: subsequence matching with scoring (similar to fzf's algorithm).
- Bonus for:
  - Consecutive character matches
  - Matches at word boundaries (after `-`, `_`, `:`)
  - Match at start of string
- Use the `nucleo` crate (same engine as Helix editor's picker) for high-performance fuzzy matching.

---

## Tool Detection & Completions

### Package Managers

Detect by lockfile presence (in priority order):

| Lockfile          | Tool   | Command prefix     |
|-------------------|--------|--------------------|
| `pnpm-lock.yaml`  | pnpm   | `pnpm `            |
| `yarn.lock`       | yarn   | `yarn `            |
| `package-lock.json`| npm   | `npm run `         |
| `bun.lockb`       | bun    | `bun run `         |

Source: `package.json` → `scripts` object. Each key is a suggestion, value is the preview.

### Make

Trigger: `make ` prefix.
Source: `Makefile` / `GNUmakefile` / `makefile`.
Parse targets: lines matching `^target-name:` that aren't variables or pattern rules.
Preview: first line of recipe (if short) or `.PHONY` annotation.

### Docker Compose

Trigger: `docker compose ` or `docker-compose ` prefix.
Source: `docker-compose.yml` / `compose.yml` / `compose.yaml`.
For `up`, `start`, `stop`, `restart`, `logs` subcommands → suggest service names.
Preview: image name from the service definition.

### Cargo

Trigger: `cargo ` prefix.
Source: `Cargo.toml` presence.
Suggest standard cargo subcommands: `build`, `run`, `test`, `check`, `clippy`, `fmt`, `bench`, `doc`.
Also parse `[package.metadata.scripts]` if using cargo-make or similar.
Preview: brief description of each subcommand.

---

## Configuration

Config file: `~/.peek/config.toml`

```toml
# Trigger behavior
trigger = "auto"  # "auto" | "tab"

# Max dropdown items
max_suggestions = 8

# Frecency tuning
[frecency]
recency_half_life_days = 7
frequency_weight = 1.0
recency_weight = 2.0

# Disable specific tools
[tools]
pnpm = true
npm = true
yarn = true
bun = true
make = true
docker_compose = true
cargo = true
```

---

## CLI Commands

```
peek init <shell>       # Print shell integration script
peek start              # Start the daemon
peek stop               # Stop the daemon
peek status             # Show daemon status, watched directories
peek config             # Open config file in $EDITOR
peek completions <tool> # List current completions for a tool in cwd
peek history            # Show frecency history for current directory
peek clear-history      # Clear frecency data
```

---

## Homebrew Distribution

### Formula

- Tap: `homebrew-peek` (or a personal tap initially).
- Formula builds the Rust binary via `cargo build --release`.
- Post-install: prints shell init instructions.
- `brew services start peek` installs and starts the launchd agent.

### launchd plist

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" ...>
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.peek.daemon</string>
  <key>ProgramArguments</key>
  <array>
    <string>/opt/homebrew/bin/peekd</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>StandardOutPath</key>
  <string>~/.peek/logs/peekd.log</string>
  <key>StandardErrorPath</key>
  <string>~/.peek/logs/peekd.err</string>
</dict>
</plist>
```

---

## Project Structure

```
peek/
├── Cargo.toml
├── crates/
│   ├── peekd/              # Daemon binary
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── server.rs       # Unix socket server
│   │   │   ├── scanner.rs      # Directory scanner
│   │   │   ├── cache.rs        # In-memory cache
│   │   │   ├── watcher.rs      # File system watcher
│   │   │   ├── frecency.rs     # Frecency scoring + SQLite
│   │   │   └── config.rs       # Config loading
│   │   └── Cargo.toml
│   ├── peek-cli/            # CLI binary
│   │   ├── src/
│   │   │   └── main.rs
│   │   └── Cargo.toml
│   ├── peek-core/           # Shared types and protocol
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── protocol.rs     # JSON message types
│   │   │   ├── tools.rs        # Tool detection logic
│   │   │   └── fuzzy.rs        # Fuzzy matching wrapper
│   │   └── Cargo.toml
│   └── peek-shell/          # Shell integration scripts
│       ├── src/
│       │   ├── lib.rs
│       │   ├── zsh.rs
│       │   ├── bash.rs
│       │   └── fish.rs
│       └── Cargo.toml
├── shell/                   # Shell script templates
│   ├── peek.zsh
│   ├── peek.bash
│   └── peek.fish
├── assets/
│   └── com.peek.daemon.plist
└── README.md
```

---

## Key Dependencies

| Crate        | Purpose                          |
|--------------|----------------------------------|
| `tokio`      | Async runtime for daemon         |
| `serde`/`serde_json` | Protocol serialization  |
| `notify`     | File system watching (kqueue)    |
| `nucleo`     | High-performance fuzzy matching  |
| `rusqlite`   | Frecency history storage         |
| `crossterm`  | Terminal rendering primitives    |
| `toml`       | Config file parsing              |
| `clap`       | CLI argument parsing             |
| `tracing`    | Structured logging               |

---

## Verification Plan

1. **Unit tests:** Scanner parsers for each tool (package.json, Makefile, compose.yml, Cargo.toml).
2. **Integration tests:** Daemon start/stop, socket communication, cache invalidation.
3. **Shell tests:** Verify `peek init <shell>` output is valid shell syntax for each shell.
4. **Manual testing:**
   - `cd` into a project with package.json → verify dropdown appears on `pnpm `.
   - Edit package.json to add a script → verify it appears without restarting.
   - Run a script multiple times → verify frecency sorting changes.
   - Test in iTerm2, Terminal.app, and at least one other terminal.
5. **Performance:** Daemon startup < 50ms, suggestion response < 10ms, memory < 20MB.
