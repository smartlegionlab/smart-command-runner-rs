# Smart Command Runner

**Run grouped shell commands from a TOML file with blocks, confirmation modes, and logging**

---

[![GitHub top language](https://img.shields.io/github/languages/top/smartlegionlab/smart-command-runner-rs)](https://github.com/smartlegionlab/smart-command-runner-rs)
[![GitHub license](https://img.shields.io/github/license/smartlegionlab/smart-command-runner-rs)](https://github.com/smartlegionlab/smart-command-runner-rs/blob/master/LICENSE)
[![GitHub release](https://img.shields.io/github/v/release/smartlegionlab/smart-command-runner-rs)](https://github.com/smartlegionlab/smart-command-runner-rs/)
[![GitHub stars](https://img.shields.io/github/stars/smartlegionlab/smart-command-runner-rs?style=social)](https://github.com/smartlegionlab/smart-command-runner-rs/stargazers)
[![GitHub forks](https://img.shields.io/github/forks/smartlegionlab/smart-command-runner-rs?style=social)](https://github.com/smartlegionlab/smart-command-runner-rs/network/members)

---

`cmdrun` reads a TOML file with named blocks of shell commands and runs them.
Blocks can be selected individually, all at once, or with per-block /
per-command confirmation. Dry-run by default — nothing runs unless you pass
`--yes`, `--interactive-block`, or `--interactive-command`.

---

## Why this tool

When setting up a fresh Linux install, you usually have a list of commands to
run — sometimes dozens, sometimes hundreds. Typing them by hand is slow and
error-prone. Shell scripts work, but you lose:

- **Named blocks** — you cannot run just "the base part" or "the Manjaro part".
- **Per-block / per-command confirmation** — for the first dry run.
- **Logs** — what ran, what failed, how long it took.

`cmdrun` solves this with a **single TOML file** that can be reused across
distributions without duplication.

---

## Origin

This project is a revival of an earlier tool called **Commandoro** (2018),
which performed a similar job: read a declarative file of grouped shell
commands and run them. The original repository was closed and the project
was abandoned. `cmdrun` is a rewrite from scratch in Rust, with a stricter
data model (atomic commands, independent blocks, explicit ordering on the
command line) and proper logging.

---

## Disclaimer

**By using this software, you agree to the full disclaimer terms.**

**Summary:** Software provided "AS IS" without warranty. You assume all risks.

**Full legal disclaimer:** See [DISCLAIMER.md](https://github.com/smartlegionlab/smart-command-runner-rs/blob/master/DISCLAIMER.md)

---

## Concepts

The tool operates on two levels of granularity.

**Command** — a single shell invocation. One command performs one action:

```toml
"sudo apt update -y"
```

**Block** — a named group of independent commands. Commands within a block
are not ordered by dependency; order is preserved for execution but does not
imply coupling:

```toml
[base]
commands = [
    "sudo apt update -y",
    "sudo apt install -y git curl wget",
]
```

### Rules

1. **Each command is atomic.** If two actions depend on each other, they are
   one command, not two. Example:

   ```toml
   # Do not split:
   # "git clone ... ~/y"
   # "cd ~/y && make install"

   # Combine:
   "git clone ... ~/y && cd ~/y && make install"
   ```

2. **Commands within a block are independent.** A failure of one command does
   not affect the others. Execution continues.

3. **Block dependencies are declared on the command line**, not inside the
   file:

   ```bash
   cmdrun --file setup.toml --run base,manjaro --yes
   ```

   The TOML file is declarative. There is no `depends_on`, no inheritance,
   no macros.

4. **Command failures are collected, not fatal.** A failed command does not
   stop the block, the file, or the process. All failures are reported at the
   end, and the process exit code is `0` unless a configuration error
   occurred.

---

## Features

- TOML file with named blocks of commands
- Run all blocks or a specific subset (`--run base,manjaro`)
- `--list` to see blocks without running anything
- Two execution modes: dry-run (default) and execute
- Three confirmation modes: `--yes`, `--interactive-block`,
  `--interactive-command` (the last two can be combined)
- Per-command output with exit status and timing
- Log file with timestamps (default: `~/.local/share/cmdrun/cmdrun.log`)
- `--no-log` to disable logging
- Non-zero exit codes of failed commands are recorded in the log
- Continues on errors — reports them at the end

---

## Installation

Requires Rust 1.85 or newer (edition 2024).

### Build from source

```bash
git clone https://github.com/smartlegionlab/smart-command-runner-rs
cd smart-command-runner-rs
cargo build --release
```

Binary: `target/release/cmdrun`

### Install for system-wide use (Linux)

```bash
mkdir -p ~/.local/bin
ln -sf "$PWD/target/release/cmdrun" ~/.local/bin/cmdrun
```

Make sure `~/.local/bin` is in your `PATH`:

```bash
echo $PATH | tr ':' '\n' | grep -q "$HOME/.local/bin" && echo "OK" || echo "NOT IN PATH"
```

If it prints `NOT IN PATH`, add to `~/.bashrc`:

```bash
export PATH="$HOME/.local/bin:$PATH"
source ~/.bashrc
```

Verify:

```bash
which cmdrun
cmdrun --version
```

---

## File format

A TOML file with one or more named blocks. Each block has:

- `description` — optional string.
- `commands` — required array of strings. Each string is one atomic command.

```toml
[base]
description = "Common setup for all Linux systems"
commands = [
    "sudo apt update -y",
    "sudo apt install -y git curl wget htop neovim",
    "mkdir -p ~/.local/bin",
]

[ubuntu]
description = "Ubuntu-specific packages"
commands = [
    "sudo apt install -y build-essential pkg-config libssl-dev",
]

[manjaro]
description = "Manjaro-specific packages"
commands = [
    "sudo pacman -Syu --noconfirm",
    "sudo pacman -S --noconfirm base-devel openssl",
]

[dev-tools]
description = "Cross-distro dev tools"
commands = [
    "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y",
    "rustup component add clippy rustfmt",
]
```

**Blocks run in this order:** the order passed to `--run`, or — if `--run` is
omitted — alphabetical order of block names.

**No `depends_on`, no inheritance, no macros.** If block B needs block A,
pass both: `--run a,b`. Explicit is better than magic.

---

## Usage

```
cmdrun --file <FILE> [OPTIONS]
```

### Options

| Option                        | Description                                                  | Default                                  |
|-------------------------------|--------------------------------------------------------------|------------------------------------------|
| `-f, --file <PATH>`           | TOML file with blocks (required)                             | —                                        |
| `-r, --run <LIST>`            | Run only these blocks, in this order (comma-separated)       | all blocks in alphabetical order         |
| `--list`                      | Show blocks in the file and exit                             | —                                        |
| `-y, --yes`                   | Run everything without asking                                | `false`                                  |
| `-i, --interactive-block`     | Ask confirmation per block                                   | `false`                                  |
| `-c, --interactive-command`   | Ask confirmation per command                                 | `false`                                  |
| `--dry-run`                   | Force dry-run even with `--yes`                              | default without `-y`/`-i`/`-c`           |
| `--log <PATH>`                | Log file path                                                | `~/.local/share/cmdrun/cmdrun.log`       |
| `--no-log`                    | Do not write a log file                                      | `false`                                  |
| `--sh <PATH>`                 | Shell used to run each command (with `-c`)                   | `sh`                                     |

### Modes

| Command line                        | Behaviour                                              |
|-------------------------------------|--------------------------------------------------------|
| `cmdrun --file X`                   | Dry-run: show plan, execute nothing                    |
| `cmdrun --file X --yes`             | Execute everything, no questions                       |
| `cmdrun --file X -i`                | Ask confirmation per block                             |
| `cmdrun --file X -c`                | Ask confirmation per command                           |
| `cmdrun --file X -i -c`             | Ask per block, then per command inside each block      |
| `cmdrun --file X --yes --dry-run`   | Still dry-run                                          |
| `cmdrun --file X -i -y`             | Error (conflict)                                       |
| `cmdrun --file X -c -y`             | Error (conflict)                                       |

### Interactive prompt

Per block:

```
Block [base] — Common setup for all Linux systems
  Commands: 3
Run this block? [y/N/a/q]
```

Per command:

```
  [1/3] [base] sudo apt update -y? [y/N/a/q]
```

Answers:

- `y` — run this one.
- `n` or Enter — skip.
- `a` — run all remaining without asking.
- `q` — quit.

### Examples

List blocks:

```bash
cmdrun --file linux-setup.toml --list
```

Dry-run (default — nothing is executed):

```bash
cmdrun --file linux-setup.toml
```

Run everything without questions:

```bash
cmdrun --file linux-setup.toml --yes
```

Run only the common setup for all systems, then Manjaro-specific:

```bash
cmdrun --file linux-setup.toml --run base,manjaro --yes
```

First run — review every block, then every command:

```bash
cmdrun --file linux-setup.toml -i -c
```

Write the log to a custom path:

```bash
cmdrun --file linux-setup.toml --yes --log ~/manjaro-setup.log
```

Run without writing a log:

```bash
cmdrun --file linux-setup.toml --yes --no-log
```

---

## Log format

Default path: `~/.local/share/cmdrun/cmdrun.log`. The file is appended to on
each run.

```
[2026-09-28T10:23:45Z] === cmdrun START file=linux-setup.toml blocks=base,manjaro
[2026-09-28T10:23:45Z] [base] [1/2] RUN: sudo apt update -y
[2026-09-28T10:23:47Z] [base] [1/2] OK (2.10s)
[2026-09-28T10:23:47Z] [base] [2/2] RUN: sudo apt install -y git curl wget htop neovim
[2026-09-28T10:23:52Z] [base] [2/2] OK (4.71s)
[2026-09-28T10:23:52Z] [base] BLOCK DONE (6.81s)
[2026-09-28T10:23:52Z] [manjaro] [1/2] RUN: sudo pacman -Syu --noconfirm
[2026-09-28T10:23:59Z] [manjaro] [1/2] OK (7.12s)
[2026-09-28T10:23:59Z] [manjaro] [2/2] RUN: some-missing-command
[2026-09-28T10:23:59Z] [manjaro] [2/2] FAIL (0.00s): exit 127: sh: some-missing-command: not found
[2026-09-28T10:23:59Z] [manjaro] BLOCK DONE (7.12s)
[2026-09-28T10:23:59Z] === cmdrun END run=3 skipped=0 failed=1
```

Successful commands are logged as `OK (Ns)`. Failed commands are logged as
`FAIL (Ns): exit N: <output>`. Non-zero exit codes are preserved. A failed
command does not stop the block — see `## Concepts → Rules`.

---

## Exit codes

- `0` — success (even if some commands failed; check the summary)
- `1` — invalid arguments, file missing, TTY required but missing, or block
  name not found

---

## Testing

```bash
cargo test
```

Runs unit tests (in `src/main.rs`) and integration tests (in
`tests/integration.rs`).

---

## License

Author: [Alexander Suvorov](https://smartlegionlab.github.io)

BSD 3-Clause License. See [LICENSE](LICENSE).

