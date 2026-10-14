# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

SQL Pro is a compact desktop SQL client (like Sequel Pro) for MySQL/MariaDB, PostgreSQL and SQLite.
Stack: Rust + Tauri v2 backend, Vue 3 + TypeScript frontend, `bun` as package manager.

## Commands

Cargo must be on PATH. If it is not, run `source ~/.cargo/env` first.

```sh
bun install
bun run tauri dev          # run the app (starts vite on :1420 and the Rust backend)
bun run tauri build        # release build

# Frontend checks
bun run lint               # eslint
bun run typecheck          # vue-tsc --noEmit
bun test                   # all tests in tests/
bun test tests/Session.test.ts            # one file
bun test -t "name of the test"            # one test by name
bun run build              # typecheck + vite build

# Backend checks (run inside src-tauri/)
cargo clippy --all-targets # must stay at zero warnings (clippy all + pedantic)
cargo test
cargo test --test service  # the end-to-end flow only
cargo fmt
```

Backend tests run on SQLite by default. MySQL and PostgreSQL run only when these are set
(`docker-compose.yml` starts local test servers on ports 3307 and 5433, password `root`):

```sh
SQLPRO_TEST_MYSQL=host:port:user:password:database
SQLPRO_TEST_POSTGRES=host:port:user:password:database
SQLPRO_TEST_SSH=host:port:user:password      # optional, runs both through an SSH tunnel
```

Format only the files you changed (`bunx prettier --write <files>`). `bun run format` rewrites the whole repo.

After changing a capability in `src-tauri/capabilities/default.json` or the icons, the dev app must rebuild.
`bun run tauri icon src-tauri/icons/source.png` regenerates all icons; delete the `android/` and `ios/` folders it creates.

## Backend (`src-tauri/src`)

Layers: `commands.rs` → `service.rs` (`ConnectionService`) → `repository/` → `model.rs`.
Constants are in `constants/mod.rs`. Errors are `AppError` (`error.rs`), which reaches the frontend as a plain string.

- **No ORM.** All SQL is written by hand with sqlx 0.8.
- **`Repository` trait** (`repository/mod.rs`) has one implementation per database (`mysql.rs`, `postgres.rs`,
  `sqlite.rs`). Shared SQL building lives in `service.rs`; the trait only holds what differs per database
  (quoting, placeholders, catalog queries, `ALTER` syntax).
- **Reads use `sqlx::raw_sql`** (text protocol), so every value comes back as `Option<String>`. This is how one
  grid can show any column type. **Writes use `sqlx::query`** with bound `Option<String>` values. PostgreSQL
  placeholders are wrapped as `CAST($n AS type)` for non-text columns.
- **One connection per tab**: each tab has its own pool with `max_connections(1)`, so `USE database` and
  transactions stay on the same connection.
- **`save_changes`** applies pending deletes, then updates, then inserts in one transaction
  (`Repository::execute_all`). This is the only write path for row data.
- Identifiers are quoted with the repository helpers. Column types and defaults in `add_column` / `create_table`
  are raw SQL from the user by design; only names are quoted.
- **SSH tunnels** (`tunnel.rs`) run the system `ssh -N -L`. A password travels only in the environment of the
  ssh process (askpass helper), never on a command line. Unix only for passwords.
- **Backups** (`dump.rs`) run `mysqldump` / `pg_dump` or copy the SQLite file. Passwords go through
  `MYSQL_PWD` / `PGPASSWORD`.
- **Saved passwords** (`password.rs`) live in the OS keychain, all in one item (a JSON vault), so macOS asks
  for permission once.
- **Closing the window hides it** (`lib.rs`). The app keeps running. macOS brings it back on Dock click;
  Windows and Linux have a tray icon and single-instance (those dependencies are non-macOS only).

### Debug and release must never share data

A debug build must not see the saved connections of the release app (they can be production databases).

- Keychain service is `SQL Pro (dev)` in debug and `SQL Pro` in release (`KEYCHAIN_SERVICE`).
- The backend injects `window.__SQL_PRO_PROFILE__` (`"dev"` or `""`). The frontend `ProfileStorage` adds
  `.dev` to every `localStorage` key. Always store through `ProfileStorage`, never `localStorage` directly.
- In debug on macOS, `run_under_dev_name` re-executes the binary under the name `SQL Pro` so the Dock label is right.

## Frontend (`src`)

Vue 3 `<script setup>`. No Pinia, no router, no UI kit.

- **Classes plus dependency injection.** Services are classes created in `main.ts` and passed down with
  provide/inject. The injection keys are in `services/keys.ts`.
- **`Session`** holds all state of one tab (connection, open table, rows, filters, pending edits, new rows,
  delete marks, query results). Its methods throw; components call `session.run(promise)`, which sets the busy
  flag and the status or error text. `SessionManager` owns the list of tabs.
- **Pending changes**: cell edits, new rows and deletes are kept in `Session` and sent only on Save
  (button, ⌘S / Ctrl+S). Anything that would lose them (switch table, refresh, sort, filter, page, close tab)
  goes through `DiscardPrompt` first.
- **`DatabaseApi`** is the only place that calls Tauri `invoke`. Tests replace it with a fake.
- **Destructive actions** (drop, truncate, drop column, delete user, backup, delete connection, save with
  deletes) always ask for confirmation with the dialog plugin. Keep this for any new one.
- **Keyboard shortcuts** are handled in `App.vue` `onKey` through `Shortcuts`. Read `metaKey` / `ctrlKey` from
  the event directly; a spread copy of a real `KeyboardEvent` loses them.
- **Zoom** is native webview zoom (`WebviewZoom`), driven by the app's own key handling. `--zoom` in CSS keeps
  the room for the macOS window buttons the same real size.
- **Design tokens** are in `style.css`: dark theme on `:root` (the default), light theme under
  `:root[data-theme='light']`. Use the tokens, not raw colours. The window has an overlay title bar, so the tab
  bar sits next to the macOS window buttons.
- Global constants go in `src/constants/`.

Tests are in `tests/` (bun test) and cover the service classes with fakes. There are no component tests.

## Code rules

- Frontend: no arrow functions (ESLint enforces it) and avoid callbacks; use classes and methods.
- Rust: edition 2024, `rust-version = "1.85"`. Because of that Cargo picks older Tauri crates, so the npm
  packages are pinned to match (`@tauri-apps/api` `~2.11`, `@tauri-apps/plugin-dialog` `~2.7`). Keep them in step.
- Passwords never appear on a command line, in logs or in error text.
- Write comments, UI text, commit messages and docs in plain, simple English with short sentences.
- New UI must work in dark and light theme and stay compact.

## Release

`.github/workflows/build.yml` builds macOS, Windows and Linux on a `v*` tag (release) or by hand
(artifacts). The release starts as a draft. The `publish` job publishes it when all builds pass.
If a build fails, the release stays a draft.
Installers are not code signed, but the update files are signed with the updater key.
`install.sh` and `install.ps1` install from the latest release. `TODO.md` holds work that was put off.

### Auto update

- The code is in `src-tauri/src/updater.rs` (Rust only, no frontend code). It checks once when the app starts,
  asks the user, then installs and restarts. Release builds only: a debug build never checks.
- The app reads `https://github.com/necessarylion/sql-pro/releases/latest/download/latest.json`
  (`plugins.updater` in `tauri.conf.json`). Users get an update only after the workflow has published the release.
- The tag must match the version in `tauri.conf.json`. Raise the version there before you tag.
- The workflow signs with the repository secret `TAURI_SIGNING_PRIVATE_KEY` (no password). The private key
  file is `~/.tauri/sql-pro-updater.key` on the owner's machine. Never commit it.
- `bundle.createUpdaterArtifacts` is on, so a local `bun run tauri build` fails at the signing step unless
  `TAURI_SIGNING_PRIVATE_KEY` is set (for example to the content of that key file).
- On Linux only the AppImage can update itself. A .deb or .rpm install shows an error popup instead.

How to ship an update:

1. Raise the version in `src-tauri/tauri.conf.json`.
2. Commit and push.
3. Push a tag like `v0.2.0`.

The workflow builds and publishes the release. Users get the popup after that.
If a build fails, the release stays a draft and users get nothing. Fix the build, or publish the draft by hand.
