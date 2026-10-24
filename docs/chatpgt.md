I compared the public repositories for [Gyotaku](https://github.com/xevrion/gyotaku?utm_source=chatgpt.com) and [Slate](https://github.com/divyanshudhruv/slate?utm_source=chatgpt.com), including their READMEs, architecture documentation, Cargo configuration, and Slate's development instructions.

My main conclusion: Slate already has a sensible high-level architecture. Its biggest gap is making the difficult Windows window-management behavior explicit, testable, and dependable. I would not copy Gyotaku's architecture wholesale. Gyotaku processes screenshots through OCR, indexing, and search, so its separate crates are natural. Slate manages real external Windows windows, so its most important architectural concerns are window ownership, lifecycle, focus, recovery, and synchronization.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://raw.githubusercontent.com\&sz=32)

GitHub

+2



## 1. What Slate already gets right

Slate's current architecture, as documented in [`AGENTS.md`](https://github.com/divyanshudhruv/slate/blob/main/AGENTS.md?utm_source=chatgpt.com), is:

GPUI UI layer

Host window, tab bar, overlays, panels

Commands (`Cmd`)

Controller

Dispatch, UI snapshot, coordination

Win32 integration

Window enumeration, hooks, host lifecycle, tray, icons, capture, updater

Core state + configuration

Groups, tabs, saved sets, update planning, persisted settings

The separation is a good start: the UI does not call Win32 directly, the controller coordinates actions, and the core state has tests. Slate also has CI for formatting, Clippy, building, and tests.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://raw.githubusercontent.com\&sz=32)

GitHub

+1



The missing piece is not necessarily more layers. It is clear contracts between these layers, explicit handling of Windows edge cases, and tests that exercise real window behavior.

# 2. Architectural gaps to address first

These are the areas I would investigate and document before expanding the feature set. I am distinguishing missing documentation or demonstrated guarantees from functionality that may already exist in the code.

## P0: Essential architecture and correctness

1\. Explicit window lifecycle state machine

Highest priority

Slate needs a clearly defined lifecycle for every hosted window. Windows can close, become invalid, get destroyed by their process, or change state outside Slate.

Document and enforce transitions such as:

`Discovered → Attaching → Hosted → Detaching → Detached`

Also handle `AttachFailed`, `ExternalClose`, `HostShutdown`, and `RecoveryRequired`.

Questions to settle:

- What happens if the external app exits while its tab is selected?
- What if attaching fails halfway through?
- Can the same HWND accidentally be registered twice?
- What happens if the host window is destroyed while a child window is still attached?

The exact states can differ, but transitions should be explicit and testable.

2\. A single authoritative window registry

The controller and Win32 layer need a clear answer to: which windows does Slate own, and what is their current state?

Establish invariants around:

- HWND identity and validity.
- The host associated with each member window.
- Whether a window is attached, detached, closing, or unavailable.
- Ownership of hooks and cleanup responsibilities.
- Synchronization between native Windows state and Slate's in-memory state.

HWND values can be reused after destruction. Avoid treating an HWND alone as a permanently valid identity. Where necessary, pair it with lifecycle information and validate it before operations.

Goal: no ghost tabs, duplicate registrations, stale handles, or orphaned hosted windows.

3\. A documented Win32 hosting contract

This is Slate's core technical differentiator. Hosting a real window is much more complicated than rendering a tab.

Document how Slate handles:

- Reparenting and restoring original window styles.
- DWM non-client frame behavior and title bars.
- Focus, keyboard input, activation and foreground-window rules.
- Z-order, topmost windows, menus, tooltips and popups.
- Minimize, maximize, restore and resize.
- DPI scaling, monitor changes, negative virtual-desktop coordinates and mixed-DPI displays.
- Windows that create additional owned dialogs or child windows.

Define the invariants for attachment and detachment. For example, detaching should restore the intended desktop behavior rather than merely remove the tab.

Slate already documents an important limitation: hosted windows can draw above Slate's own menus and dialogs. That should become part of a systematic compatibility model, not remain an isolated caveat.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://github.com\&sz=32)

GitHub

+1



4\. Recovery and rollback guarantees

Slate already has saved sets, configuration backups and a self-update fallback. Those are useful foundations, but the guarantees should be stated precisely.

Specify:

- Whether a failed attachment rolls back every native change.
- What happens if a member window or Slate crashes.
- How a saved set behaves when some applications are closed.
- Whether shutdown restores all surviving hosted windows.
- How settings and saved sets survive partial writes or malformed JSON.
- What happens if an update fails during replacement or restart.

A particularly important distinction: saved sets currently restore windows that are already running. They are not a process launcher or a full session-restoration system. Keep that contract explicit unless you deliberately expand it.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://github.com\&sz=32)

GitHub

+1



## P1: Architecture that will prevent future problems

### 5. Define a native-event and synchronization strategy

Slate's documented flow is UI → command → controller → Win32 → core state.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://raw.githubusercontent.com\&sz=32)

GitHub

&#x20;But Windows can change independently of a user command.



For example, an external application can close itself while Slate is idle. The architecture should specify how those native events update the controller's snapshot and UI.

Define:

- Which thread owns Win32 operations and native hooks.
- How native callbacks are forwarded to the controller.
- Whether events are queued, coalesced or processed immediately.
- How stale events are discarded.
- How polling, if any, reconciles missed events.
- How the UI is refreshed without excessive work.

Recommendation: keep the core state deterministic and testable, and treat native Windows events as inputs to the state machine rather than allowing multiple layers to mutate state independently.

### 6. Make commands transactional and failure-aware

Commands should not just be actions; they should have explicit outcomes.

For example:

```
AttachWindow(hwnd)
  → ValidateWindow
    → CaptureOriginalState
      → ApplyHostConfiguration
        → RegisterHostedWindow
          → PublishSnapshot
          ```

          If a native operation fails midway, the command should have a defined rollback or recovery outcome.

          A useful command contract should describe:

          - Preconditions.
          - State changes.
          - Possible Windows API failures.
          - Rollback behavior.
          - Whether the command is retryable.
          - What the UI should display after failure.

          You don't necessarily need a generic transaction framework. Just make the risky operations explicit.

          ### 7. Establish a boundary between pure core and platform code

          Slate already separates `src/core`, `src/config`, `src/controller`, `src/win` and `src/ui`. That is a solid structure.&#x20;

          [image](https://www.google.com/s2/favicons?domain=https://raw.githubusercontent.com\&sz=32)

          GitHub



          I would formalize the dependency rules:

          | Layer        | Should own                                         | Should not own                                            |
          | ------------ | -------------------------------------------------- | --------------------------------------------------------- |
          | `core`       | Tab/group state, transitions, saved-set semantics  | HWND calls, GPUI types                                    |
          | `config`     | Serialization, validation, migration               | Window manipulation                                       |
          | `controller` | Command dispatch, coordination, published snapshot | Rendering or raw UI logic                                 |
          | `win`        | Win32 operations, native events, lifecycle         | Product-level tab state as an independent source of truth |
          | `ui`         | Rendering, input, menus, visual feedback           | Direct Win32 operations                                   |

          The exact module names can remain as they are. The important thing is to enforce the boundaries in code review and tests.

          ### 8. Make persistence a versioned contract

          Slate currently stores configuration, settings, saved sets, installation metadata and updater files under `%USERPROFILE%\.burntfern`.&#x20;

          [image](https://www.google.com/s2/favicons?domain=https://github.com\&sz=32)

          GitHub

          +1



          For long-term reliability, define:

          - A schema version for each persisted format.
          - Forward migration rules when settings change.
          - Behavior for unknown fields and invalid values.
          - Atomic writes using a temporary file and replacement where appropriate.
          - When backups are created and how many are retained.
          - Whether corrupt saved sets can be recovered independently of settings.
          - Which data is disposable cache versus user-owned configuration.

          Also document which state is session-only. Per-tab accent colors are currently described as session-only, so users should not assume they survive restarting Slate.&#x20;

          [image](https://www.google.com/s2/favicons?domain=https://github.com\&sz=32)

          GitHub

          +1



          ### 9. Isolate updater behavior from normal application startup

          Self-updating software is a sensitive part of a Windows desktop application, especially when the executable must replace itself.

          Slate already has a separate `--janitor` path and documents a replacement fallback.&#x20;

          [image](https://www.google.com/s2/favicons?domain=https://raw.githubusercontent.com\&sz=32)

          GitHub

          +1



          The updater contract should spell out:

          1. How the release is discovered.
          2. How the downloaded artifact is validated.
          3. Where temporary files are written.
          4. How the running process exits or hands off control.
          5. How replacement and restart are performed.
          6. How failed startup triggers rollback.
          7. When old executables and temporary files are deleted.

          Important: make sure artifact integrity is actually enforced before execution or replacement, not merely documented as something a user can check manually. If signatures are unavailable, checksum verification must be tied to a trustworthy release source. A checksum distributed beside an untrusted binary does not independently establish authenticity.

          # 3. What Gyotaku does better as an engineering reference

          Gyotaku's standout advantage is not its number of crates. It is how explicitly it documents engineering decisions and validates them against real workloads.

          Its [architecture document](https://github.com/xevrion/gyotaku/blob/main/docs/architecture.md?utm_source=chatgpt.com) describes the processing pipeline, platform differences, performance optimizations, failure cases and repository structure. Its `AGENTS.md` records correctness rules and bugs the project has already encountered.&#x20;

          [image](https://www.google.com/s2/favicons?domain=https://raw.githubusercontent.com\&sz=32)

          GitHub

          +1



          Here is what I would borrow for Slate.

          | Engineering practice        | Gyotaku's approach                                                      | What Slate should adopt                                                                          |
          | --------------------------- | ----------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
          | Architecture documentation  | End-to-end data flow and repository map                                 | A dedicated `docs/architecture.md` with the Win32 lifecycle and command flow                     |
          | Explain tradeoffs           | Records why OCR engines, indexing and rendering choices were made       | Document why Slate uses native window hosting, and the limitations of that choice                |
          | Performance evidence        | Publishes measurements and methodology                                  | Measure idle RAM, startup time, tab-switch latency and CPU usage                                 |
          | Failure handling            | Explicit rules for broken configs, missing GPU and unavailable services | Define behavior for stale HWNDs, failed attachment, app exits and updater failures               |
          | Platform abstraction        | Operating-system differences are isolated behind platform modules       | Keep Windows-specific code behind clear interfaces, even if Windows is currently the only target |
          | Testable boundaries         | Core and OCR crates can be tested independently of the UI               | Expand pure-core tests and add native Windows integration tests                                  |
          | Compatibility documentation | Supported and unverified environments are stated                        | Publish a real compatibility matrix for applications and Windows configurations                  |

          Gyotaku even documents a performance issue caused by GPUI's image loader and why it replaced that path. The lesson is valuable: record the actual failure, the measured impact, and the chosen workaround, rather than just documenting what the code currently does.&#x20;

          [image](https://www.google.com/s2/favicons?domain=https://raw.githubusercontent.com\&sz=32)

          GitHub

          +1



          # 4. What's missing or vague in Slate's public documentation?

          The README is already useful for users. It explains the core value proposition, keyboard shortcuts, installation, saved sets, file locations and known limitations.&#x20;

          [image](https://www.google.com/s2/favicons?domain=https://github.com\&sz=32)

          GitHub

          +1



          But it is primarily a user guide, not an architecture or reliability specification.

          ## Documentation checklist

          0 of 10 checked

          Reset

          Architecture overview

          Explain each layer, its responsibilities, and allowed dependencies.

          Window lifecycle

          Define attachment, detachment, closure, rollback and shutdown behavior.

          Win32 compatibility

          Document styles, focus, z-order, dialogs, DPI and known problematic applications.

          Native event flow

          Explain hooks, event delivery, reconciliation and stale-handle handling.

          Persistence specification

          Define schemas, migrations, atomic writes, recovery and session-only settings.

          Updater threat model

          Explain download validation, replacement, rollback and cleanup.

          Testing strategy

          Separate pure-core tests, mocked Win32 tests and real Windows integration tests.

          Performance methodology

          State hardware, Windows version, workload, measurement method and results.

          Compatibility matrix

          Track supported, tested, problematic and untested applications and environments.

          Troubleshooting

          Cover failed attachment, missing tabs, elevated apps, crashes, scaling and update failures.

          ## What I would put in `docs/architecture.md`

          Start with these sections, in this order:

          1. System overview: a diagram of the UI, controller, core, config and Win32 layers.
          2. State ownership: who owns the tab registry, native handles and published UI snapshot.
          3. Attach/detach lifecycle: step-by-step sequences, invariants, failure points and rollback.
          4. External window events: how close, minimize, focus and destruction events reach the controller.
          5. Persistence: settings and saved-set formats, validation and migration behavior.
          6. Startup, shutdown and recovery: tray lifecycle, single-instance behavior and crash handling.
          7. Updates: download validation, replacement, rollback and cleanup.
          8. Testing and compatibility: what is tested automatically, what needs a real Windows machine, and what remains unverified.

          That would make Slate much easier for another developer or coding agent to change safely.

          # 5. The tests Slate needs most

          The public development instructions say `cargo test` currently covers `src/core` and `src/config`; the UI and Win32 layers require running Slate on Windows.&#x20;

          [image](https://www.google.com/s2/favicons?domain=https://raw.githubusercontent.com\&sz=32)

          GitHub



          That is an understandable starting point, but the native integration boundary is precisely where the highest-risk bugs live.

          Pure core tests

          Test invalid state transitions, duplicate tabs, saved-set serialization, partially missing windows, tab ordering and command failure outcomes.

          Win32 integration tests

          Create real test windows and verify attach, detach, focus, resize, close, restore and shutdown behavior.

          Failure-injection tests

          Simulate invalid handles, a target closing during attachment, native API errors, corrupted configuration and failed updates.

          Real desktop smoke tests

          Run the actual release build, interact with hosted windows, exercise multiple monitors where available, and verify screenshots or other observable results.

          Do not make every test depend on the desktop. Keep most state-machine and configuration tests fast, and reserve real native integration tests for behavior that cannot be faithfully simulated.

          ## Compatibility cases to explicitly test

          - Notepad, File Explorer, terminal windows and ordinary Win32 applications.
          - Apps with custom title bars or unusual window styles.
          - Apps that open modal dialogs, owned popups or separate utility windows.
          - Apps that minimize to the notification area.
          - Apps that exit unexpectedly.
          - Elevated applications and mismatched integrity levels.
          - Mixed-DPI displays and monitors with different resolutions.
          - Multiple Slate windows or repeated attachment attempts.
          - Sleep, resume, display changes and explorer/taskbar restarts, where applicable.

          The goal is not to claim universal compatibility. It is to know where Slate works, where it does not, and whether failures leave applications usable.

          # 6. What is missing from the product contract?

          These questions matter as much as the internal code because they define what users should expect.

          | Question                                               | What Slate should decide and document                                                                                                    |
          | ------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------- |
          | Can Slate launch apps?                                 | Currently no. Is that a permanent design choice or a later feature?                                                                      |
          | What happens after a reboot?                           | Does Slate start automatically, and do saved sets restore automatically or only manually?                                                |
          | What happens when an app closes?                       | Does its tab disappear immediately, and how is its saved-set entry treated?                                                              |
          | Can users host multiple windows from the same process? | Define behavior for multi-window applications and duplicate detection.                                                                   |
          | How are elevated apps handled?                         | Document privilege boundaries and whether running Slate elevated is supported and recommended.                                           |
          | Which windows are unsupported?                         | Define exclusions and communicate why they are excluded.                                                                                 |
          | What does closing Slate mean?                          | Clarify whether closing the host hides it, exits the process, or restores all windows.                                                   |
          | What survives a crash?                                 | Distinguish session state, saved sets and configuration.                                                                                 |
          | What does "saved set" mean?                            | Clarify whether it saves window identities, app paths, ordering, tab accents, placement or just a snapshot of currently running windows. |

          Do not implement all of these as features immediately. Decide the contract first. In particular, launching applications and restoring complete workspaces would be meaningful scope expansions, not merely small architectural fixes.

          # 7. A practical order of work

          If I were preparing Slate for a serious public launch, I would prioritize the work like this.

          ## 1

          Specify the architecture

          Write the architecture document, state transitions, native-window invariants and persistence contract. Identify any contradictions between the documented design and actual implementation.

          ## 2

          Harden the window lifecycle

          Fix or add handling for external closure, failed attachment, detachment rollback, stale handles, host shutdown and synchronization of native events.

          ## 3

          Expand test coverage

          Add core transition tests and a Windows test harness for real native windows. Use failure injection for the hard-to-reproduce cases.

          ## 4

          Audit updater and persistence

          Verify integrity checks, rollback behavior, atomic writes, schema migrations and cleanup. Test interruption and failure paths.

          ## 5

          Measure and document real behavior

          Gather startup, idle memory, tab-switch and recovery measurements, then publish the test methodology and compatibility matrix.

          ## 6

          Polish the user experience

          Make failure states understandable, improve troubleshooting and onboarding, and refine the website and demo once the behavior is dependable.

          ## My final assessment

          Slate does not need to become a large workspace-management platform to be a well-engineered project. Its existing UI/controller/core/Win32 split is a reasonable foundation.

          What it needs to become excellent is:

          - A formally understood window lifecycle, including failure and recovery.
          - One authoritative model of hosted windows, synchronized with real Windows events.
          - A clearly specified Win32 compatibility contract, especially for focus, z-order, dialogs and DPI.
          - Automated tests for the actual native behavior, not just the pure state model.
          - Documented and verified persistence, updater and shutdown guarantees.
          - Measured performance and a truthful compatibility matrix.

          I have reviewed the public repository and its published development instructions, but I have not executed Slate or audited every implementation path. So I would treat the items above as a prioritized architecture review, not a claim that each behavior is currently broken or absent.

          The best next step is to write `docs/architecture.md` and turn each high-risk lifecycle invariant into a test. That will give you a concrete definition of “perfect” for Slate: not a codebase with the most layers, but one whose behavior remains predictable when real Windows applications do unexpected things.
