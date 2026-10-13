# SQL Pro

A small desktop SQL client, like Sequel Pro. It works with MySQL, PostgreSQL and SQLite.
It is built with Tauri v2 (Rust backend) and Vue 3 + TypeScript (frontend).

## Run

```sh
bun install
bun run tauri dev
```

## Checks

```sh
bun run lint       # ESLint
bun run typecheck  # vue-tsc
bun run format     # Prettier
bun test           # Session logic with a fake backend
```

## Layout

```
src/
  main.ts          builds the objects and hands them to the components (provide/inject)
  types.ts         types shared with the backend
  constants/       all global constants
  services/
    DatabaseApi    the only file that calls the backend (Tauri invoke)
    Session        all state and logic of one tab
    SessionManager the list of tabs
    FavoriteStore  saved connections (passwords go to the OS keychain)
    keys           the injection keys
  components/      the views. They only show state and call Session methods.
  style.css        global colours, light and dark
src-tauri/         the Rust backend
```

The flow is: component -> `Session` -> `DatabaseApi` -> Rust command.

## Code rules

- No arrow functions. Use classes, class methods and named functions. ESLint checks this.
- Classes get what they need in the constructor.
- `Session` methods throw on failure. Components wrap them in `session.run(...)`.
  `run` sets `busy` and stores the error text in `error`.
