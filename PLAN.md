# docker-tui — Implementation Plan

A terminal UI for browsing Docker Hub, pulling images, inspecting local
containers (including their filesystems) and opening shells inside them.

Development is test-driven: each feature starts with failing tests (pure logic,
parsers, state transitions, rendered-screen snapshots against mock backends),
followed by the implementation that makes them pass.

## Progress

- [x] Phase 0 — Skeleton
- [x] Phase 1 — Local Docker (read)
- [x] Phase 2 — Lifecycle & logs
- [x] Phase 3 — Hub & pull
- [x] Phase 4 — Filesystem & shell (MVP)
- [x] Phase 5 — Embedded terminal
- [x] Phase 6 — Polish

### Deviations from the original plan

- **Hub search runs on Enter** instead of search-as-you-type with debounce,
  which keeps anonymous Hub rate limits out of reach.
- **Refresh is periodic** (configurable interval) rather than driven by the
  `/events` stream.
- **Shell detection** stats candidate paths through the archive endpoint, so it
  works in images without `test`/`which`. `x` hands the terminal to
  `docker exec -it` (the `docker` CLI must be on `PATH`); `X` opens the
  embedded pane.
- Deferred: image inspect (`i`), copying a file out to the host, keybinding
  remapping, themes, a run-container form, registry auth, and mouse support.

### Running

Common tasks are wrapped in the `Makefile` (`make help` lists them all):

```
make run ARGS="--host unix:///var/run/docker.sock"   # debug build + run
make release                                         # target/release/docker-tui
make test                                            # unit + snapshot tests (tests/unit)
make test-integration                                # against the real Docker daemon
make clippy / make fmt / make doc                    # lint, format, API docs
make check                                           # pre-commit: fmt-check, clippy, tests
make ci                                              # everything CI runs
```

Integration tests only create and remove their own `dtui-test-*` containers
(from `alpine:3.20`, pulled if missing).

`~/.config/docker-tui/config.toml` (all keys optional):

```toml
host = "unix:///var/run/docker.sock"  # --host and DOCKER_HOST win
refresh_secs = 2
shell = "/bin/zsh"                     # tried before bash/sh
log_tail = 500
```

### Key bindings

| Where | Keys |
|---|---|
| Everywhere | `1 2 3`/`Tab` tabs · `?` help · `P` pulls panel · `q`/`Ctrl-c` quit |
| Lists | `j k ↑ ↓ g G PgUp PgDn` move · `r` refresh |
| Hub | `/` search · `Enter` tags · `p` pull latest / selected tag · `Esc` back |
| Images | `d` remove |
| Containers | `Enter` details · `s` start/stop · `R` restart · `d` remove · `a` all/running |
| Container (list or detail) | `x` shell (full screen) · `X` shell pane · `:` run command |
| Detail | `h l ← →` Info/Logs/Stats/Files · `Esc` back |
| Logs | `j k` scroll · `G`/`f` follow |
| Files | `Enter` open dir/file · `Bksp`/`-` up · `D` changes (diff) · `r` reload |
| Shell pane | every key goes to the shell · `Ctrl-\` give focus back · `X` refocus |

## 1. Tech stack

| Concern | Crate | Why |
|---|---|---|
| TUI rendering | `ratatui` + `crossterm` | De-facto standard, mature widgets |
| Async runtime | `tokio` | Docker streams, HTTP, UI loop concurrently |
| Docker Engine API | `bollard` | Native socket API, typed models, streaming pull/logs/exec/stats |
| Docker Hub API | `reqwest` (rustls) + `serde` | Plain REST/JSON |
| Text input | `tui-input` | Search boxes, command prompt |
| Embedded terminal (phase 5) | `vt100` + `tui-term` | Render a container shell inside a pane |
| Tar parsing | `tar` | Reading container files via the archive endpoint |
| Errors / logging | `color-eyre`, `tracing` + `tracing-appender` | Log to file (stdout belongs to the TUI) |
| Config | `serde` + `toml` + `directories` | Keybindings, default shell, socket path |
| Testing | `insta`, ratatui `TestBackend` | Snapshot-test rendered screens |

## 2. Architecture

Unidirectional message loop (Elm-style):

```
 ┌──────────── main loop (tokio::select!) ────────────┐
 │  terminal events ─┐                                │
 │  tick (250ms) ────┼──► Msg ──► App::update(msg)    │
 │  task results ────┘            │                   │
 │                                ├─► mutate state    │
 │                                └─► spawn Command   │
 │  App::view(frame)  ◄── redraw when dirty           │
 └────────────────────────────────────────────────────┘
         ▲ mpsc::UnboundedSender<Msg>
         │
  background tasks: Docker API calls, Hub HTTP, pull progress
  streams, log/stats streams, exec sessions
```

- The UI never awaits I/O. `App::update` is synchronous and returns
  `Command`s; a runtime executes them on `tokio::spawn` and posts results back
  as `Msg`s. This makes `update` fully unit-testable.
- Long-running streams (logs, stats, pull progress) are cancellable via
  `CancellationToken` when leaving the view.
- `DockerApi` and `HubApi` are traits so the UI is tested against mocks.

### Module layout

```
src/
  main.rs            # bootstrap: terminal init/restore, panic hook, runtime
  lib.rs
  app.rs             # App state, update(), view()
  msg.rs             # Msg / Command enums
  event.rs           # crossterm EventStream + tick → Msg
  runtime.rs         # executes Commands against the APIs
  config.rs
  docker/
    mod.rs           # DockerApi trait + bollard impl
    models.rs        # app-level container/image/etc. models
    fs.rs            # ls parsing, tar listing, diff
    exec.rs          # shell detection, exec helpers
    pull.rs          # pull progress folding
  hub/
    mod.rs           # HubApi trait + reqwest impl
    models.rs        # search results, tags
  ui/
    mod.rs           # tabs, status bar, help popup
    hub.rs
    images.rs
    containers.rs
    container_detail.rs   # Info | Logs | Files | Stats
    pull_progress.rs
    terminal.rs      # embedded shell pane (phase 5)
```

## 3. Screens & UX

Top bar: **[1] Hub  [2] Images  [3] Containers**; bottom bar: context keys and
toasts; `?` help.

1. **Hub** — `/` search, results table (name, stars, pulls, official badge,
   description), details pane; `Enter` → tag list; `p` pull selected tag.
2. **Images** — local images table (repo:tag, id, size, created); `d` delete
   (confirm), `i` inspect.
3. **Containers** — list (name, image, status, ports), toggle all/running (`a`);
   `s` start/stop, `R` restart, `d` remove, `Enter` detail view.
   - Detail sub-tabs: **Info** (inspect), **Logs** (follow), **Files** (browser),
     **Stats** (sparklines).
   - `x` open shell, `:` run one-off command.
4. **Pull progress** — per-layer progress bars from the `create_image` stream;
   pulls continue in background; status bar shows active count.

## 4. Key technical decisions

### Docker Hub API
- Search: `GET https://hub.docker.com/v2/search/repositories/?query=<q>&page=<n>&page_size=25`
- Tags: `GET /v2/repositories/{ns}/{repo}/tags?page_size=50&ordering=last_updated`
  (paginate via `next`); official images live under `library`.
- Debounce input, in-memory cache, friendly 429 handling. Parsing isolated in
  `hub/models.rs` so endpoints can be swapped cheaply.

### Pulling
`create_image` stream items folded into `PullState { layers: IndexMap<id, LayerProgress> }`.

### Filesystem browsing
1. **Exec-based (primary)**: `ls -la <path>` via exec, parsed (GNU & BusyBox).
2. **Archive-based (fallback)**: `GET /containers/{id}/archive?path=…` tar —
   works for stopped/distroless containers. Used for single-file preview
   (capped, binary detection) and directory listing (headers only, depth 1,
   entry cap).
- Extras: `docker diff` view, copy file out to host.

### Shell access
- **Phase A (MVP)**: suspend TUI and run `docker exec -it <id> <shell>` with
  inherited stdio; restore after exit. Shell detection: `/bin/bash` →
  `/bin/sh` → `/busybox/sh`.
- **Phase B**: embedded pane — bollard exec with TTY, `vt100::Parser`,
  `tui_term::PseudoTerminal`, resize via `resize_exec`, `Ctrl-\` to leave.
- **Quick command runner** `:` — non-interactive exec, output in a popup.

### Terminal safety
Panic hook + guard restore raw mode / alternate screen / cursor.

### Connection
`Docker::connect_with_defaults()` (respects `DOCKER_HOST`), `--host` flag;
friendly error screen on socket permission issues.

## 5. Phased delivery

| Phase | Scope | Done when |
|---|---|---|
| **0. Skeleton** | deps, terminal init/restore, panic hook, event loop, Msg/App, tabs, help popup, file logging | empty tabs render, `q` exits cleanly, panics don't corrupt terminal |
| **1. Local Docker (read)** | connection, containers & images lists, periodic refresh, inspect view | lists match `docker ps -a` / `docker images` |
| **2. Lifecycle & logs** | start/stop/restart/rm, image rm, confirm dialogs, toasts, logs view w/ follow, stats sparklines | daily ops from the TUI |
| **3. Hub & pull** | Hub search/tags, background pulls with per-layer progress, refresh images after pull | search `nginx` → pick tag → pull → appears in Images |
| **4. Filesystem & shell (MVP)** | exec-based browser + archive fallback, preview, diff view, suspend-and-handoff shell, `:` runner | browse `/etc` in alpine & distroless; open shell and return |
| **5. Embedded terminal** | vt100 pane, resize, split view | vim/htop work inside pane |
| **6. Polish** | config file, `--host`, CI (fmt/clippy/test) | v0.1 |

## 6. Testing strategy

Tests live outside `src/`, so production code carries no test modules:

```
tests/
  unit/          # one test crate mirroring src/: tests/unit/app/hub.rs tests src/app/hub.rs
    main.rs
    support/     # MockDocker, MockHub, model builders, key(), render_to_string()
    ui/snapshots/
  docker_integration.rs   # opt-in, against the real daemon
  fixtures/      # captured Hub responses and `ls` output
```

They go through the crate's public API; private helpers are covered through
the public functions that use them.

- **Unit**: parsers (`ls` output, tar listing, Hub JSON fixtures, pull progress
  folding), `App::update` state transitions (msg in → state + commands out).
- **UI snapshots**: render screens into `TestBackend`, `insta` snapshots.
- **Integration (opt-in)**: `cargo test -- --ignored` against the real daemon
  using `alpine` containers created/torn down by the test.

## 7. Risks
- Hub API changes / rate limits → isolated client, cache, friendly errors.
- No shell / no `ls` in container → archive fallback, clear message.
- TTY handoff glitches → always restore via guard, drain input after return.
- Huge dirs/files → caps on entries & preview size; streaming.
