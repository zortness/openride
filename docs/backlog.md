# Product backlog

Items here capture agreed future requirements without assigning an implementation milestone.

## Native authoring and agent workflows

See [tooling direction](tooling-roadmap.md), recorded during the 2026-10-11 source
publication audit. Proposed: incrementally migrate production Python authoring to
Rust while retaining parity tests, then remove the end-user Python dependency.
Build/verify and activity-intake skills are recommended; only shared AGENTS.md
guidance has been added so far. This migration is separate from M6 hardware work.

## Free riding and controller route choices

Recorded 2026-10-11. Deferred beyond [M6 device compatibility](m6-device-compatibility.md).
M6 establishes device-independent shift, direction-choice, confirm/back and
pause/menu actions; it does not make nearby rendered roads traversable.

- Build a connected, routable road network with junction choices, branch elevation
  and road geometry, and content coverage beyond the original route corridor.
- Offer left/right/straight choices with a visible countdown and default continuation
  so riders without controllers can continue riding.
- Preserve ordinary shifting on two-button controllers; use an explicit temporary
  junction-selection mode or configurable gesture, not ambiguous shared presses.
- Defer free steering and the exact junction interaction design until this work.
- Treat an Android rider app, lightweight companion, and Echelon screen sideloading
  as separate feasibility efforts. None is required to investigate Echelon bike BLE.

## Rider and bike profiles

Status: deferred for future consideration, requested 2026-10-10. Profile management is not required for the current M4 implementation slice.

Requirements:

- Persist multiple named rider profiles locally, including rider mass.
- Persist multiple named bike profiles, including complete outdoor bike/equipment mass (excluding the trainer).
- Allow selection of rider/bike combinations for a session; avoid a single global rider or bike assumption. A rider can use different bikes, and a bike can be selected by different riders.
- Compute total simulation mass from the selected rider and bike values. Support reviewing and updating estimates over time.
- Save the selected profile identities and a snapshot of the actual parameter values with each recorded activity, so later profile edits do not change historical ride context.
- Keep virtual-physics parameters distinct from trainer-specific configuration and trainer-difficulty settings. Device support for transmitting mass must be established separately.

Future design questions: ownership/sharing of bike profiles, remembered/default combinations, equipment-load overrides, units, and additional rider/bike physics parameters. Local profiles do not require cloud accounts.

Related work: [M4 implementation](m4-progress.md). The current power demo uses an explicit 85 kg combined-mass assumption; adding these profiles is a separate future task.
# Longer route authoring

- Replace the initial 5 km section cap with tiled terrain and OSM acquisition,
  resolution budgets, resumable per-tile preparation, and continuous stationing
  and review decisions across tile boundaries. Existing runtime chunk streaming
  is only the rendering part of this work. Keep source/data provenance per tile.

## Application navigation and activity history

Requested 2026-10-11 after successfully authoring and installing Red Rock.
The current import inspector is not a useful application home; launching a route
library popup above it hides the main flow. Recorded sessions are not discovered
by its temporary imported-activity browser.

Direction: one main application window, focused screens for major operations,
fewer popups, visible Back navigation and consistent Escape behavior. User cites
Forza Horizon 6 as a reference for simplifying complex menus. Full-window screens
should work both in windowed and fullscreen display modes.

Implementation update (2026-10-11): the exported desktop app now uses a single
Control-based application shell with Home/Ride/Create/Activities/Settings screens.
Route creation, history/exports, 3D editing/preview, readiness and riding occupy the
main window. Existing journals are discovered incrementally; dates are currently
shown in UTC. Back retains editor state and opens a guarded menu during live rides.
Controller bindings, focus styling, stick repeat/dead zones and disconnect reset
are implemented. Verification uses simulated controller input and a fake trainer;
the user confirmed most physical controller navigation works. Follow-up fixes add
file-dialog shoulder focus traversal, left-stick editor orbit and contextual inner
tabs. Full physical ride-flow validation and controller model documentation remain.
Text entry and file paths still use the keyboard/desktop picker, explicitly noted
in Settings. Full binding-remap UI, controller text entry, richer activity graphs,
and a multi-project recent-project catalog remain follow-up UX work.

Navigation structure:

- Home: prominent Ride action, recent routes, recent activities, resume project.
- Ride: installed routes, route details, device readiness, start ride.
- Create: saved projects, import, acquisition, repair, full-window 3D preview.
- Activities: automatically discover local recorded sessions, including older
  sessions; summary, simple graphs and existing FIT/TCX/GPX/CSV export actions.
  Distinguish completed, interrupted and test sessions using recorded evidence.
- Settings: devices, units and preferences; diagnostics under an advanced area.
- During rides, replace navigation tabs with the riding HUD and a ride menu.
  Escape opens that menu; returning home requires the existing pause/end/recording
  lifecycle, not destroying a live controller or silently ending the activity.

Navigation requirements:

- Back from preview restores its originating project or route selection, including
  scroll position, filters, editor cursor and camera where appropriate.
- Preserve authoring drafts and background acquisition/build jobs across screen
  changes. Confirm only actual unsaved-data loss or active-ride termination.
- Use transient dialogs for file selection and short confirmations; route creation,
  activity history, ride setup, 3D inspection and riding occupy the main content area.
- Controller navigation is a first-class requirement, requested 2026-10-11:
  a gamepad is easier to keep beside or on the bike than a keyboard and mouse.
  Include it from the first application-shell slice rather than adding it after
  the screen migration. Keyboard and mouse remain supported.
- Use D-pad/left stick for predictable focus movement, a consistent confirm action,
  and a consistent Back action. Show a strong focus indicator and contextual button
  prompts; switch prompts to match the active input method. Avoid hover-only actions.
- Give every screen sensible initial focus, remember focus when returning, scroll
  focused items into view, and prevent focus from escaping into hidden screens.
  Provide repeat delay and stick dead zones to avoid accidental double actions.
- Make route selection, device readiness, starting a ride, opening the ride menu,
  pause/resume, ending/saving a ride, and reviewing its summary usable entirely
  with a controller. Back during a ride opens the ride menu; it must not silently
  end recording or tear down the trainer connection.
- Support controller reconnection without losing navigation or ride state.
  Use Godot input actions so bindings can be remapped and prompts need not assume
  one controller brand. Pairing/connection status must remain understandable.
- Authoring also needs controller-accessible controls, including point selection,
  road alternatives and camera movement. Define alternatives for drag/hover
  interactions. Text entry and native file dialogs need an explicit input strategy;
  do not claim complete controller support while they require an unannounced keyboard.
- Verify the routine ride flow with a physical controller and no keyboard/mouse:
  launch, select a route, review devices, ride, pause/resume, finish/save, and find
  the activity. Also check Back behavior, focus restoration, long lists, dialogs,
  and disconnect/reconnect. Identify the tested controller and connection type.
- Keep the existing imported-file inspector as a source inspection tool under
  Create, distinct from the athlete's recorded activity history.

Recommended slices:

1. Application shell, Home and embedded route library; remove the startup popup.
2. Local activity discovery, summaries and integrated export; show older recordings.
3. Full-window project workflow and 3D editor/preview with stateful return navigation.
4. Full-window ride setup, active ride and finish summary; recorded rides appear in
   Activities immediately. Exercise device cleanup, interrupted sessions and Back.

Technical direction: a small navigation controller and Control-based screens;
use SubViewportContainer/SubViewport for isolated 3D worlds as needed. Keep device
and recording lifecycle outside disposable screens. Refactor reusable content out
of Window subclasses rather than merely maximizing every popup.
