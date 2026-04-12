# Runtime And Data Flow

This document explains what happens from process start to shutdown and how data moves through the application while it is running.

## Startup Sequence

The top-level binary entry point is `src/main.rs`, but the runtime orchestration now lives in `src/services/runtime/mod.rs`.

The startup order is deliberate:

1. parse CLI arguments with `clap`
2. build `Config` from CLI plus platform defaults
3. run one-time migration of old data/log directories
4. initialize tracing to a log file
5. reject non-interactive terminal startup
6. install a panic hook that restores the terminal
7. spawn detectors and create `AppState`
8. initialize `ratatui`
9. enter the main event/render loop

This order matters for two reasons:

- logging must be ready before meaningful startup work happens
- terminal safety must be established before raw mode and alternate screen are entered

## Config And Environment

`src/config.rs` defines the runtime configuration model.

Important configuration responsibilities:

- poll interval
- log file path
- local state roots for Claude, Codex, and OpenCode
- Codex management directories
- Claude management directories

The app currently prefers convention over complex configuration. The docs and code assume default local state roots unless explicitly overridden through CLI-supported options.

## Tracing And Logging

Tracing is initialized early and writes to a file instead of stdout/stderr. That is not only a convenience decision. It is necessary because writing logs to stdout would corrupt the TUI.

The runtime behavior is:

- `RUST_LOG` controls the filter level
- logs are written to a file under the app cache directory by default
- ANSI is disabled for the log writer

If tracing initialization fails, startup fails. That is a reasonable trade-off because partial startup without reliable logging would make TUI failures much harder to diagnose.

## Terminal Lifecycle

The terminal lifecycle has three critical pieces:

### Interactive terminal checks

Before starting the TUI, the app verifies that stdin and stdout are terminals. This prevents confusing behavior when a user accidentally pipes the binary or tries to run the interactive TUI in a non-terminal context.

### `ratatui::init()`

Initialization enters the TUI mode. The terminal is then explicitly cleared once so stale alternate-screen content from previous sessions does not flash before the first render completes.

### Restoration

Restoration happens in two places:

- a panic hook calls `ratatui::restore()`
- normal shutdown calls `ratatui::restore()` after the main loop exits

That redundancy is intentional. TUI applications should assume they will be interrupted in unpleasant ways and design for shell recovery.

## Detector Topology

Detector startup is centralized in `detect::spawn_detectors()`.

The topology is:

- provider-specific detector tasks are spawned
- process scanning runs as its own async task
- a forwarding/enrichment task receives raw events and may emit additional session-info events
- the main runtime loop receives the final event stream through an `mpsc::Receiver<AgentEvent>`

The runtime does not directly poll external provider state from the UI loop. It consumes a channel of events.

## Main Loop

The main loop in `src/services/runtime/mod.rs` is the operational center of the binary.

Each iteration does four things:

1. drain detector events and apply them to `AppState`
2. drain async quota-refresh results and update profile quota state
3. expire transient status messages
4. render the current frame and process one input action

This means the application is not driven by a centralized reducer tick. Instead, it is a hybrid:

- asynchronous producers feed events and quota updates
- a synchronous render/input loop applies them and redraws the terminal

That approach fits the app well because external state changes are irregular while rendering and keyboard handling are naturally loop-based.

## Quota Polling

Quota polling is intentionally separate from detector flow.

Two background loops are spawned:

- one for Codex profiles
- one for Claude profiles

Those loops:

- enumerate profile directories
- load per-profile auth/credentials
- perform provider-specific quota refresh work
- send `(profile_id, Option<QuotaSnapshot>)` back over channels

The main loop interprets `None` as a fetch failure and clears the loading indicator without pretending new data exists.

Important architectural point: quota refresh is not treated as a detector concern. It is a profile-management concern.

## Event Flow

The high-level flow is:

1. detectors observe external state
2. detectors emit `AgentEvent`
3. `AppState::handle_event` interprets the event
4. render code reads the new state
5. input produces actions that mutate state again

The event layer is the main normalization boundary between provider-specific observation logic and provider-agnostic presentation.

## Degraded Paths

The application has several best-effort or degraded behaviors that are architecturally important:

- missing or failing process-scanner dependencies now produce detector-health events
- missing auth/credential files do not crash quota polling; they skip profile refresh
- undocumented or internal upstream quota endpoints can fail silently at the network/API layer but clear loading state
- provider schema drift can degrade detection without breaking the whole TUI process

The design goal is to degrade the relevant feature instead of crashing the application.

## Shutdown Behavior

Normal shutdown happens when:

- the user presses quit
- the loop breaks due to a termination signal on Unix

Background tasks are mostly fire-and-forget. They are not drained with explicit cancellation tokens. Instead, the current design relies on the tokio runtime dropping on process exit, which aborts in-flight work.

That is acceptable here because:

- this is a local terminal application
- most background work is observation or best-effort quota refresh
- the app does not own a durable background job queue that needs graceful draining

It is still a trade-off. If the application gains more consequential background side effects later, shutdown coordination will need to become more explicit.

## Where To Be Careful

Changes in this layer can break the whole user experience even if business logic stays correct.

Be especially careful with:

- terminal initialization/restoration ordering
- panic paths
- any stdout/stderr writes during TUI mode
- blocking calls inside the main loop
- changes that make degraded states invisible to the user
