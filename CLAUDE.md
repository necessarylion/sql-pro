# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

SQL Pro is a compact desktop SQL client (like Sequel Pro) for MySQL/MariaDB, PostgreSQL, SQL Server and SQLite.
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

Backend tests run on SQLite by default. MySQL, PostgreSQL and SQL Server run only when these are set
(`docker-compose.yml` starts local test servers on ports 3307 and 5433 with password `root`, and SQL Server on
port 1434 with user `sa` and password `Root1234!`):

```sh
SQLPRO_TEST_MYSQL=host:port:user:password:database
SQLPRO_TEST_POSTGRES=host:port:user:password:database
SQLPRO_TEST_MSSQL=host:port:user:password:database   # the test creates the database when it is missing
SQLPRO_TEST_SSH=host:port:user:password      # optional, runs them through an SSH tunnel
```

The PostgreSQL backup check needs a `pg_dump` with the same major version as the server (17 for Docker).
With Homebrew: `PATH="/opt/homebrew/opt/libpq@17/bin:$PATH" cargo test`.

Format only the files you changed (`bunx prettier --write <files>`). `bun run format` rewrites the whole repo.

After changing a capability in `src-tauri/capabilities/default.json` or the icons, the dev app must rebuild.
`bun run tauri icon src-tauri/icons/source.png` regenerates all icons; delete the `android/` and `ios/` folders it creates.

## Backend (`src-tauri/src`)

Layers: `commands.rs` → `service.rs` (`ConnectionService`) → `repository/` → `model.rs`.
Constants are in `constants/mod.rs`. Errors are `AppError` (`error.rs`), which reaches the frontend as a plain string.

- **No ORM.** All SQL is written by hand with sqlx 0.9. sqlx only takes dynamic SQL inside
  `AssertSqlSafe`.
- **`Repository` trait** (`repository/mod.rs`) has one implementation per database (`mysql.rs`, `postgres.rs`,
  `mssql.rs`, `sqlite.rs`). Shared SQL building lives in `service.rs`; the trait only holds what differs per database
  (quoting, placeholders, catalog queries, `ALTER` syntax).
- **Reads use `sqlx::raw_sql`** (text protocol), so every value comes back as `Option<String>`. This is how one
  grid can show any column type. **Writes use `sqlx::query`** with bound `Option<String>` values. PostgreSQL
  placeholders are wrapped as `CAST($n AS type)` for non-text columns.
- **One connection per tab**: each tab has its own pool with `max_connections(1)`, so `USE database` and
  transactions stay on the same connection. A tab and its copies share one spare connection
  (`service/spare.rs`) that opens in the background. A new tab takes it, so it opens at once.
- **`save_changes`** applies pending deletes, then updates, then inserts in one transaction
  (`Repository::execute_all`). This is the only write path for row data.
- Identifiers are quoted with the repository helpers. Column types and defaults in `add_column` / `create_table`
  are raw SQL from the user by design; only names are quoted.
- **SSH tunnels** (`tunnel.rs`) run the system `ssh -N -L`. A password travels only in the environment of the
  ssh process (askpass helper), never on a command line. Unix only for passwords.
- **SQL Server** (`mssql.rs`) uses our own fork of the driver: `https://github.com/necessarylion/sqlx-sqlserver`,
  a git dependency on branch `main` (`Cargo.lock` pins the commit). Fix driver bugs in the fork (local clone
  `../sqlx-sqlserver`), push `main`, then run `cargo update -p sqlx-sqlserver`. Never open PRs upstream.
  Limits of the driver and how the app works around them:
  - It cannot read `xml`, `text`, `ntext`, `image`, `sql_variant` or CLR types (`geography`, ...). Browse and
    export cast them on the server (`Repository::select_column`). The query editor shows an error with a hint.
  - It gives no values as `Option<String>`. `MssqlRepository::text` turns each type into the text SQL Server
    itself writes (decimals keep their scale, binary is `0x…`, GUIDs in upper case).
  - It has no cancel. Cancel runs `KILL <spid>` from a second connection. That ends the session, so the tab
    opens a new connection for its next query.
  - It reads its settings only from a URL. `encrypt=mandatory&trust_server_certificate=true`: encrypted, but the
    certificate is not checked, like the other drivers. `PRINT` messages are dropped.
  - Paging is `OFFSET … FETCH` (`Repository::page_sql`). Rename uses `sp_rename`. A default is a constraint, so
    changing or dropping a column drops its default first.
  - Backup and the Users view are not there for SQL Server yet. See `TODO.md`.
- **Backups** (`dump.rs`) run `mysqldump` / `pg_dump` or copy the SQLite file. Passwords go through
  `MYSQL_PWD` / `PGPASSWORD`.
- **Saved passwords** (`password.rs`) live in the OS keychain, all in one item (a JSON vault), so macOS asks
  for permission once.
- **Closing the window hides it** (`lib.rs`). The app keeps running. macOS brings it back on Dock click;
  Windows and Linux have a tray icon and single-instance (those dependencies are non-macOS only).
- **Open a SQLite file from the OS** (`open_file.rs`, `bundle.fileAssociations`). macOS sends the paths in
  `RunEvent::Opened`. Windows and Linux pass them as program arguments, or to the single-instance callback.
  The paths wait in `OpenedFiles` until the frontend takes them (`take_opened_files`). The `open-files` event
  tells a page that already runs. `FileOpener` opens each file in a tab, without saving it as a connection.
  The file types are only registered by an installed build, so this does not work in `tauri dev`.

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
- Rust: edition 2024, `rust-version = "1.99"`. The npm Tauri packages must have the same minor version as the
  crates in `Cargo.lock`: `@tauri-apps/api` and `@tauri-apps/cli` `~2.12` (crate `tauri`),
  `@tauri-apps/plugin-dialog` `~2.8` (crate `tauri-plugin-dialog`). When you update one side, update the other.
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
- `latest.json` is made once by the `publish` job with `.github/scripts/latest-json.sh`, from the `.sig` files
  in the release. The build jobs do not upload it (`uploadUpdaterJson: false`), because six jobs writing one
  file lose platforms.
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
