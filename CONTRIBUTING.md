# Contributing to SQL Pro

Thank you for your help. This page tells you how to set up the project and what we expect in a pull request.

## Set up

You need [bun](https://bun.sh) and [Rust](https://rustup.rs). On Linux you also need the
[Tauri packages](https://tauri.app/start/prerequisites/).

```sh
bun install
bun run tauri dev
```

The file `CLAUDE.md` describes how the code is built. Read it before a bigger change.

## Run the checks

Run these before you open a pull request. CI runs the same checks.

```sh
# Frontend (in the repository root)
bun run lint
bun run typecheck
bun test

# Backend (in src-tauri/)
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

- Clippy must have zero warnings.
- Format only the files you changed: `bunx prettier --write <files>`. `bun run format` rewrites the whole repository.

## Test policy

- A bug fix comes with a test that fails without the fix.
- New logic in a `Session` or a service class comes with a test in `tests/`. Use the fake `DatabaseApi`.
- New backend logic comes with a test in `src-tauri/tests/`. These tests run on SQLite by default.
- Behavior that differs per database should also run on MySQL, PostgreSQL or SQL Server.
  `docker-compose.yml` starts these servers. The environment variables are in `CLAUDE.md`.
  CI runs SQLite only, so run the other databases on your machine when you change their code.
- There are no component tests. Keep logic out of the components and in the classes, so it can be tested.
- A change that is only text or style does not need a test.

## Code rules

- Frontend: no arrow functions (ESLint checks this). Avoid callbacks. Use classes and methods.
- Write comments, UI text and docs in plain, simple English with short sentences.
- Passwords must never appear on a command line, in logs or in error text.
- New UI must work in the dark and the light theme.
- Ask for confirmation before any destructive action.

## Report a bug

Open an issue at https://github.com/necessarylion/sql-pro/issues. Please write:

- The app version and your operating system.
- The database and its version (for example PostgreSQL 17).
- The steps to reproduce the bug.
- What you expected to happen.
- What happened instead.

Do not paste passwords or production data. Remove them from logs and screenshots.
For a security problem, do not open an issue. Follow [SECURITY.md](SECURITY.md).

## Pull requests

- Keep a pull request small and about one thing.
- Say the problem first, then what it causes, then how your change fixes it.
- Wait for CI to pass.
- Do not commit secrets or the updater key.

## Security

Do not open a public issue for a security problem. See [SECURITY.md](SECURITY.md).
