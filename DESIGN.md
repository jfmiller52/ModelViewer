---
name: ModelView
description: Fast local STL and 3MF printability inspector.
colors:
  black: "#000000"
  ash: "#05080C"
  ash-high: "#0A1220"
  white: "#FFFFFF"
  graphite: "#A7B3BF"
  graphite-faint: "#5C6B78"
  hairline: "#161E28"
  hairline-bright: "#263545"
  blue: "#2F6BFF"
  blue-dim: "#0A1730"
  scroll-hover: "#2A3B4E"
  scroll-active: "#344B62"
  signal-red: "#FF5A5F"
  signal-red-text: "#FF8A8A"
  amber: "#F5A524"
  teal: "#14B8A6"
  violet: "#A78BFA"
  red-line: "#D92D20"
  amber-line: "#B45309"
  teal-line: "#0F766E"
  violet-line: "#6D28D9"
  clay-light: "#D3DCE6"
  clay-blue: "#9DB2C8"
  clay-mist: "#C2CCD8"
  clay-slate: "#7891A8"
typography:
  wordmark:
    fontFamily: "Archivo Narrow, sans-serif"
    fontSize: "76px"
    fontWeight: 600
    lineHeight: 1.15
    letterSpacing: "0.03em"
  verdict:
    fontFamily: "Archivo Narrow, sans-serif"
    fontSize: "44px"
    fontWeight: 500
    lineHeight: 1.05
    letterSpacing: "0.05em"
  section-label:
    fontFamily: "Archivo Narrow, sans-serif"
    fontSize: "11.5px"
    fontWeight: 600
    lineHeight: 1.2
    letterSpacing: "0.19em"
  row-label:
    fontFamily: "Archivo Narrow, sans-serif"
    fontSize: "15px"
    fontWeight: 500
    lineHeight: 1.2
    letterSpacing: "0.013em"
  body:
    fontFamily: "Archivo, sans-serif"
    fontSize: "14.5px"
    fontWeight: 400
    lineHeight: 1.35
  data:
    fontFamily: "Archivo Narrow, sans-serif"
    fontSize: "18px"
    fontWeight: 500
    lineHeight: 1.1
    letterSpacing: "0.022em"
  control:
    fontFamily: "Archivo Narrow, sans-serif"
    fontSize: "12.5px"
    fontWeight: 600
    lineHeight: 1
    letterSpacing: "0.128em"
  meta:
    fontFamily: "Archivo Narrow, sans-serif"
    fontSize: "11.5px"
    fontWeight: 400
    lineHeight: 1.2
    letterSpacing: "0.148em"
rounded:
  none: "0px"
spacing:
  hairline-gap: "2px"
  xs: "4px"
  sm: "6px"
  md: "8px"
  lg: "14px"
  xl: "20px"
  section: "18px"
  row: "46px"
  data-row: "48px"
  object-row: "54px"
  rail: "76px"
  inspector: "440px"
  topbar: "66px"
  statusbar: "38px"
components:
  action-primary:
    backgroundColor: "{colors.blue}"
    textColor: "{colors.white}"
    rounded: "{rounded.none}"
    padding: "9px 18px"
    height: "36px"
  action-outline:
    backgroundColor: "{colors.black}"
    textColor: "{colors.white}"
    rounded: "{rounded.none}"
    padding: "9px 18px"
    height: "36px"
  view-segment:
    backgroundColor: "{colors.black}"
    textColor: "{colors.graphite}"
    rounded: "{rounded.none}"
    size: "58px 34px"
  tag-chip:
    backgroundColor: "{colors.black}"
    textColor: "{colors.white}"
    rounded: "{rounded.none}"
    padding: "6px 9px"
    height: "26px"
  check-row:
    backgroundColor: "{colors.black}"
    textColor: "{colors.white}"
    height: "46px"
  object-row:
    backgroundColor: "{colors.black}"
    textColor: "{colors.white}"
    height: "54px"
  rail-tick:
    backgroundColor: "{colors.black}"
    textColor: "{colors.graphite}"
    width: "76px"
    height: "58px"
  defect-callout:
    backgroundColor: "{colors.black}"
    textColor: "{colors.signal-red-text}"
    rounded: "50%"
    size: "28px 28px"
  settings-sheet:
    backgroundColor: "{colors.black}"
    textColor: "{colors.white}"
    borderColor: "{colors.hairline-bright}"
    rounded: "{rounded.none}"
    width: "540px"
    padding: "28px"
  repair-sheet:
    backgroundColor: "{colors.black}"
    textColor: "{colors.white}"
    borderColor: "{colors.hairline-bright}"
    rounded: "{rounded.none}"
    width: "540px"
    padding: "28px"
  slicer-radio:
    backgroundColor: "{colors.black}"
    textColor: "{colors.white}"
    rounded: "{rounded.none}"
    size: "16px 16px"
  repair-checkbox:
    backgroundColor: "{colors.black}"
    textColor: "{colors.white}"
    rounded: "{rounded.none}"
    size: "16px 16px"
  verdict-actions:
    backgroundColor: "{colors.black}"
    textColor: "{colors.white}"
    rounded: "{rounded.none}"
    height: "36px"
  repaired-tag:
    backgroundColor: "{colors.black}"
    textColor: "{colors.white}"
    borderColor: "{colors.hairline-bright}"
    rounded: "{rounded.none}"
    height: "26px"
  defect-theater:
    backgroundColor: "{colors.black}"
    textColor: "{colors.white}"
    borderColor: "{colors.hairline-bright}"
    rounded: "{rounded.none}"
    size: "260px 184px"
  topology-loupe:
    backgroundColor: "{colors.black}"
    textColor: "{colors.white}"
    borderColor: "{colors.hairline-bright}"
    rounded: "{rounded.none}"
    size: "260px 184px"
  progressive-load:
    backgroundColor: "{colors.black}"
    textColor: "{colors.white}"
    rounded: "{rounded.none}"
  analysis-filmstrip:
    backgroundColor: "{colors.black}"
    textColor: "{colors.white}"
    borderColor: "{colors.hairline-bright}"
    rounded: "{rounded.none}"
  orientation-adviser:
    backgroundColor: "{colors.black}"
    textColor: "{colors.white}"
    borderColor: "{colors.hairline-bright}"
    rounded: "{rounded.none}"
  bed-grid:
    backgroundColor: "{colors.black}"
    borderColor: "{colors.hairline-bright}"
    rounded: "{rounded.none}"
  recovery-offer:
    backgroundColor: "{colors.black}"
    textColor: "{colors.white}"
    borderColor: "{colors.hairline-bright}"
    rounded: "{rounded.none}"
    width: "540px"
---

# Design System: ModelView

## Overview

**Creative North Star: "The Blueprint Plate"**

ModelView is a working inspection plate drawn in pure black and white, with one electric blue line for interaction and four defect inks for truth. The mesh is the specimen; the calibrated index rail holds the verdict, checks, measurements and objects; a defect never borrows the blue, so the accent can never be confused with a failure. The interface answers one question — is this printable? — in a single 44px reading before the eye reaches the detail.

The surface is black at every layer: viewport, top strip, rail, inspector and status bar are all `#000000`. Separation is earned by 1px `#161E28` hairlines and `#263545` interactive borders, never by tonal panel fills. The one blue (`#2F6BFF`) marks buttons, borders, focus, active segments, the selected-object rule and the hidden-inspector hint; it is the live line. Red `#FF5A5F`, amber `#F5A524`, teal `#14B8A6` and violet `#A78BFA` mean open edge, non-manifold, flipped winding and degenerate — on the mesh, in the legend, and in the callouts.

The type ramp is deliberately larger than the first release for high-resolution displays: 76px wordmark, 44px verdict, 18px data, 15px row labels, 12.5px explanations, 11.5px metas, 10px rail labels. Density stays high because the content is dense; legibility comes first. The earlier charcoal/bone/sodium surface was retired at the user's request; the rail, callout and timing structures survive.

Two Blueprint sheets carry the handoff. Settings lists detected slicers as square radio rows and a custom Browse path; Repair pairs before/after counts with five square checkboxes, a filled Keep action, and a Send that also fills when a slicer is configured. Both are opaque black plates with a 1px Hairline Bright border, 0px corners and no shadow; they sit over the report, not inside it. A live repair is marked by a `REPAIRED PREVIEW` viewport chip; after Keep, `REPAIRED` replaces it. The chips are white like every other instrument tag, never a new accent.

Defect Theater is the one authored performance. The camera flies to a chosen defect, the specimen becomes a ghost, the focused ink and a White focus frame hold the eye, and a topology loupe reads the local topology. It adds motion and focus, not new checks or a new palette.

Large binary STL files open without waiting: the raw mesh streams into the GPU, a flat-shaded specimen appears on the first chunks, and the authoritative checks settle over a live analysis filmstrip. The Orientation Adviser is a side-section preview on an assumed 256 × 256 × 256 mm bed: it ranks candidate poses, rotates the specimen onto the bed, and never writes the file.

**Key Characteristics:**
- Pure `#000000` field everywhere, with no tonal panel fills.
- White primary text, blue-gray secondary; graphite-faint is decoration only.
- One electric blue for interactive borders, buttons, focus and selection.
- Four defect inks with fixed meanings, and darker line variants for the lit mesh.
- Square hairlines, 0px corners, no shadows; the GPU mesh supplies the depth.
- Condensed, tracked Archivo Narrow; a 44px verdict as the one display moment.
- Real parse and check timings; no idle FPS.
- Settings and Repair sheets are opaque black plates, not cards.
- Repair is explicit: `REPAIRED PREVIEW` while previewing, `REPAIRED` after Keep; the source file is never overwritten.
- One performance mode: Defect Theater ghosts the specimen, keeps one defect story in ink, and frames it in White; the report stays live.
- No-wait inspection: large binary STL shows a live flat-shaded mesh and a filmstrip of checks before the authoritative report lands.
- Orientation Adviser is a preview on an assumed 256³ bed; it never claims to have changed the source.

## Colors

A black-and-white inspection plate with one blue signal line and four calibrated defect inks.

### Primary
- **Electric Blue** (#2F6BFF): filled primary actions, outlined action borders, active view segments and plate groups, focus rings, the selected-object rule, and the inspector-hidden hint. Used on interactive state only.
- **Blue Dim** (#0A1730): the low fill behind an active plate button; never a page background.

### Secondary
- **Signal Red** (#FF5A5F): open-edge ink, large failure verdicts, and the 2px failure rule.
- **Signal Red Text** (#FF8A8A): small red text at 11–13px, where the deeper red falls under 4.5:1.
- **Amber** (#F5A524): warning states and the non-manifold defect ink.
- **Teal** (#14B8A6): the flipped-winding defect ink; distinct from the UI blue.
- **Violet** (#A78BFA): the degenerate-face defect ink.
- **Line inks** (#D92D20, #B45309, #0F766E, #6D28D9): one step darker for the 1px defect hairlines drawn on the lit white mesh; the legend and markers keep the bright inks.

### Neutral
- **Black** (#000000): the viewport, chrome, rail, inspector and status bar. There is no second background tone.
- **Ash** (#05080C): hover fill on rows and the active segment; visually black.
- **Ash High** (#0A1220): pressed state.
- **White** (#FFFFFF): primary text, data values, the wordmark and the 44px verdict on a passing file.
- **Graphite** (#A7B3BF): secondary content, explanations, metas and labels; the floor for readable text on black.
- **Graphite Faint** (#5C6B78): decorative ticks, disabled legend items and placeholder marks only. Never explanatory copy.
- **Hairline** (#161E28): structural rules.
- **Hairline Bright** (#263545): interactive borders and the scrollbar rest state.
- **Scroll Hover / Active** (#2A3B4E / #344B62): quiet scrollbar state steps.

### Material
- **Clay Light** (#D3DCE6), **Clay Blue** (#9DB2C8), **Clay Mist** (#C2CCD8), **Clay Slate** (#7891A8): the four cool object tints in scene order. They keep multi-object 3MF files distinct without returning to warm clay when no part color is defined. Defined 3MF part colors replace the cool tint.

### Named Rules
**The One Blue Rule.** `#2F6BFF` marks interactive state only: primary buttons, interactive borders, focus, active segments, selection. It is never a background, a heading color, or a defect ink.

**The Defect Ink Rule.** Red, amber, teal and violet mean open edge, non-manifold, winding and degenerate — in that binding, on the mesh and in the legend. They never decorate UI.

**The Pure Black Rule.** Every surface is `#000000`. Separation comes from a 1px hairline, never from a lighter panel fill.

**The Quiet Chrome Rule.** Content text is Graphite or White. Graphite Faint is reserved for ticks, disabled states and placeholder marks.

## Typography

**Display Font:** Archivo Narrow Semi (600) with system sans fallback
**Body Font:** Archivo (400) with system sans fallback
**Label Font:** Archivo Narrow Semi (600), tracked and uppercase

**Character:** A condensed industrial grotesque at a generous size. The narrow face carries the 44px verdict, the data plate and the tracked labels; Archivo's wider body face carries the few sentences that need reading. The ramp is one step larger than the first release to hold up on high-DPI displays.

### Hierarchy
- **Wordmark** (600, 76px, 1.15, 0.03em): the first-run title sheet only; uppercase and tracked.
- **Verdict** (500, 44px, 1.05, 0.05em): the one display moment on a loaded file — WATERTIGHT, NOT WATERTIGHT, READ FAILED, NO SPECIMEN.
- **Section label** (600, 11.5px, 0.19em): section locators, tags, rail labels and chip text; uppercase.
- **Row label** (500, 15px, 0.013em): check names, object names, the file name.
- **Body** (400, 14.5px, 1.35): report sentences and error messages; Archivo, not narrowed.
- **Data** (500, 18px, 0.022em): bounding box, volume, surface, triangle and vertex counts.
- **Control** (600, 12.5px, 0.128em): button labels and view segments; uppercase.
- **Meta** (400, 11.5px, 0.148em): format, size, timings, state lines and sublines.

Numeric values are right-aligned in the narrow face, so counts and measurements read as a column without relying on a monospaced font.

### Named Rules
**The Small Type Rule.** In the loaded report, the smallest content is 10px (rail labels); explanations are 12.5px, metas 11.5px, row labels 15px, data 18px. Display type is the 44px verdict and the 76px wordmark; nothing else exceeds 18px.

**The Tracked Caps Rule.** Labels are uppercase, tracked 0.1–0.19em; body prose is never uppercase.

## Layout

Fixed chrome around a flexible viewport: 66px top strip, 38px status bar, 76px index rail, 440px inspector. At 1440×900 the viewport is 924px wide; at 1024×700 it is 508px and the scale bar moves under the axis gizmo while the bottom navigation hint hides. The empty state is two-column at 900px and wider (480px title sheet + 300px drop target), and a single title column below that.

The inspector scrolls. Section rhythm is 18px before each head, an 18px head, then 8px; check rows are 46px, object rows 54px, data rows 48px, file rows 32px, all separated by 1px hairlines. On a multi-object 3MF, OBJECTS comes before MODEL and the rail labels reorder; on a single STL, MODEL comes first. Nothing is a card; the rail, rules and type carry the grouping.

### Named Rules
**The Fixed Chrome Rule.** The rail and inspector keep their widths. Only the viewport grows.

**The One Fold Rule.** The verdict, checks, and the first object/model group fit as far as the height allows; the rest scrolls behind a quiet blue-gray scrollbar.

## Elevation & Depth

There are no shadows. The page is a flat black field; depth is the GPU viewport — a cool clay mesh (or defined 3MF part colors), a 4× MSAA edge, a blue-tinged floor grid and a soft key/fill/rim light. The help overlay is an opaque black panel with a 1px blue-gray border over a dimmed viewport, not a floating card.

### Named Rules
**The Flat Plate Rule.** No box shadows, blur, glass or glow. A surface earns separation with a 1px line or a blue border.

## Shapes

Every UI corner is 0px. Recurring marks are 1px hairlines, 2px verdict/error rules, 2px blue active underlines, 14px visibility squares, 26px tag chips, and 28px defect-marker circles. Circles appear only as map-like defect locators and the rail's settings gear glyph; the blue line is always a rule, border or underline. The plate frame uses corner ticks rather than rounded panels.

### Named Rules
**The Square Specimen Rule.** If a corner is visible, it is 0px. The one round content form is the defect callout; the settings gear is the only other circular glyph.

## Components

### Buttons
- **Shape:** square, 1px border, 36px tall; 12.5–13px tracked caps.
- **Primary:** Electric Blue fill with white text. Used for the Open action while no file is ready, the error state's primary recovery, Keep repaired, and a configured send when it is the page's one action.
- **Outline:** black fill, Electric Blue border, white text, and a 16px Electric Blue dash before the label. This is the loaded-state Open action and the secondary recovery.
- **Hover / Focus:** hover near-black Ash; keyboard focus is a 1px Electric Blue ring outside the control.

### Verdict Actions
- **Placement:** the verdict block on a loaded file ends with the handoff pair, inside the 440px inspector.
- **Pass:** a configured slicer is one filled `SEND TO <NAME>` button; with no slicer the same filled button reads `SET UP SLICER`.
- **Fail:** a filled `REPAIR…` sits beside an outlined send action; the pair splits the available width with an 8px gap.
- **Wording:** the send action names the configured slicer, truncating long names; the blueprint never says "print" or "slice" for the handoff.

### Settings Sheet
- **Shape:** 540px content with 28px inner margin, opaque black, 1px Hairline Bright border, 0px corners, no title bar, no shadow, no fade; centered over a dimmed frame (the viewport, inspector and chrome recede under a flat black scrim).
- **Anatomy:** `SETTINGS / SLICER` locator, one 14.5px body line, detected and current slicer rows, `BROWSE…` and `RESCAN` outlined actions, a current-selection line, a "Changes save when you close ModelView." line (or a session-only line when eframe storage is unavailable), and a filled Close. The rail gear and `Cmd/Ctrl+,` open it; Esc closes.
- **Slicer row:** 56px, full width, 1px hairline, 16px square radio at the left. Name at 15px, path at 11px Graphite, `FOUND` at 9.5px Graphite at the right (or `MISSING` in Signal Red Text when the path no longer exists). The selected row fills Blue Dim, takes an Electric Blue border and a filled blue radio square. A valid custom choice renders as its own row; a vanished custom path reads `CURRENT / … — MISSING`.
- **No detection:** the sheet says so plainly and leaves Browse as the guaranteed path; there is no empty card.

### Repair Sheet
- **Shape:** 540px content on the same opaque black plate, centered; no title bar or shadow. The sheet is a diagnostic instrument, not a wizard. The topbar, rail and inspector dim behind it, while the live preview and status readout stay lit.
- **Anatomy:** `REPAIR / <file>`; `BEFORE` and `AFTER` metric lines (Open, Winding, Degenerate, Non-manifold, Shells); five 40px option rows; one disclaimer; the Keep/Revert and Save/Send rows; Close. The AFTER values are white when the preview is watertight and Signal Red Text when it still fails. `NO SAFE REPAIR APPLIED` appears when no pass changed anything; Keep is disabled in that state.
- **Repair option row:** 40px, 16px square checkbox, 12.5px label, right-aligned 11px count. Hover and click; keyboard focus is the 1px Electric Blue ring. Rows are disabled (Graphite Faint, no focus) while the preview job runs; changing a row starts a new preview. Weld is locked on for STL, because an unindexed STL cannot be safely unwelded; it reads in Graphite, not the disabled Faint.
- **Viewport state:** the repaired mesh renders behind the sheet with a `REPAIRED PREVIEW` chip; the side report switches to the preview analysis, while the original stays retained for Revert. After Keep the chip becomes `REPAIRED`.
- **Actions:** Keep repaired is filled blue; Revert and Save repaired copy… are outlined. Send is filled when a slicer is configured and outlined with `SET UP SLICER` otherwise. Save defaults to `<stem>.repaired.<ext>`, keeps the source format, and never overwrites the source.
- **Safety:** the disclaimer names the fan patch and states that non-manifold edges stay unresolved and self-intersections are not checked. The sheet never claims those were fixed.

### REPAIRED Tags
- **Style:** the same 26px viewport chip as GRID ON and DEFECTS: opaque black, 1px Hairline Bright border, 0px corners, 9px tracked white caps.
- **States:** `REPAIRED PREVIEW` while the repair sheet holds a live preview; `REPAIRED` after Keep. The tag is state, never Electric Blue, and it disappears when the next file loads.

**The Explicit Repair Rule.** Repair is previewed on the real scene; Keep is the only way to mark `REPAIRED`, and the source file is never overwritten. A no-op preview says so and cannot be kept.

**The Safe Claim Rule.** The repair reports only the five safe passes. Non-manifold edges remain reported and self-intersections are not checked; the interface never implies otherwise.

**The Defined Part Color Rule.** A 3MF's own part colors replace the cool palette. Dark print colors are lifted 32% toward white for the black plate; the object-row swatch shows the source color; undefined parts keep the Clay order. Color is per object, never invented per face. A warm part color is user content, not a sodium accent.

**The Recovered-Not-Rewritten Rule.** Recovery resolves external parts in memory, keeps plates and names, and skips broken parts. It never writes the source and never claims the mesh is watertight; Send exports a temporary recovered copy.

**The No-Wait Rule.** A large binary STL must put geometry on screen before analysis finishes. The stream is flat and provisional; the filmstrip and counts settle as the authoritative report lands. Never replace it with an indeterminate spinner.

**The Assumed Bed Rule.** Orientation is a 256 × 256 × 256 mm preview. The bed assumption is stated in the section and at the bed edge. The source file, source orientation and side report are never rewritten.

**The Sheet Focus Rule.** Settings and Help dim the frame behind them; Repair dims the topbar, rail and inspector but keeps the live preview and status readout lit. The dim is a flat black scrim, never a shadow, blur or glass. When a centered sheet would clip the fixed topbar, status bar or inspector, that chrome yields to a blank field for the sheet's lifetime; panel widths, the rail and the live preview stay put.

### Load Recovery
- **Offer:** only for 3MF packages whose object geometry is in external production parts, or whose object graph cannot resolve. The error state turns Amber and offers `FIX ON LOAD`; `OPEN ANOTHER` is the secondary. There is no sample action in this state.
- **Loading:** `RECOVERING / RESOLVING EXTERNAL OBJECT PARTS` in Amber, the same plate frame and a sweep. The source path stays visible; no false progress percentage.
- **Result:** `Metadata/model_settings.config` is applied after recovery, preserving plate ids/names and object names; broken or missing parts are skipped and reported. The scene gets a `RECOVERED` chip and the status notice reports objects/plates/skipped parts. The source file is untouched; Send exports a temporary recovered 3MF. Normal checks and Repair still run.
- **Never:** do not silently drop a package, and do not mark a recovered scene as clean; recovery resolves package structure, not mesh topology.

### Progressive Load
- **Entry:** binary STL above 100k triangles enters a streaming path. The viewport shows the raw flat-shaded specimen as chunks arrive; the camera follows the growing bounds until the user orbits, pans or zooms.
- **Live field:** the grid is absent; tags read `STL`, `LARGE MODEL`, `STREAMING` (or `VERIFYING`) and the triangle count; the side report becomes a live scan with six provisional counts. The final smooth scene replaces the raw stream only after the authoritative analysis.
- **Filmstrip:** a black strip above the bottom edge, six cells across: OPEN EDGE, NON-MANIFOLD, WINDING, VOLUME, SHELLS, DEGENERATE. Each cell is 7px label / 11px count; a 2px Electric Blue scan line crosses the strip at the progress fraction; `ANALYSIS FILMSTRIP / LIVE` and `n / N TRIANGLES · %` sit above it. At 1024 labels shorten to NON-MANIF and DEGEN.
- **No spinner:** geometry first, counts second; a percentage and live cells replace an indeterminate waiting panel.

### Orientation Adviser
- **Placement:** the sixth rail tick, `ORIENT`, opens a side section below FILE; no sheet, so the model and bed stay visible.
- **Bed:** 256 × 256 × 256 mm, assumed and labelled. The bed is a dim GPU plane at z=0 with blue axes; the object is centred on it. A `BED 256 × 256 MM / ASSUMED` label sits at bottom left.
- **Anatomy:** a body line, `FIND BEST ORIENTATION` filled action, or up to three candidate rows after analysis: `RECOMMENDED`, `LOWEST PROFILE`, `MOST CONTACT`, `FEWEST OVERHANGS` as applicable. Each row shows height, footprint, overhang and contact, with a `FITS` / `OVER` tag. The selected row fills Blue Dim with an Electric Blue border.
- **Verdict:** `FITS BED` or `NO BED FIT` in White or Signal Red Text. The pose animates over 0.55s with a camera dolly; the source mesh is untouched. `RESET POSE` returns to the loaded pose; `RE-ANALYSE` returns to the candidate list.
- **Safety:** preview only; the slicer applies orientation. The side report keeps the original checks, bounds and defect facts.

### Chips and Tags
- **Style:** 26px tall, black fill, Hairline Bright border, white tracked caps; square. The viewport row reports format, object/plate count, the STL millimetre assumption, grid state, defect count and the inspector-hidden hint. The active/inspector-hidden hint switches to Electric Blue text; the millimetre assumption stays Graphite.

### Check Rows
- **Shape:** 46px, full width, 1px bottom hairline.
- **Anatomy:** a state mark on the calibrated rail (solid white pass, dashed amber warning, doubled red failure, dim info), a 15px label, a 12.5px Graphite explanation, and a right-aligned 13.5px value.
- **Interaction:** hover Ash; click, Enter or Space focuses the first matching defect; focus is a 1px Electric Blue ring and the row scrolls into view.

### Part Colors
- **Source:** Bambu 3MF part colors come from `Metadata/model_settings.config` (`extruder` per object) matched to `Metadata/project_settings.config` (`filament_colour`). Objects without a definition keep the cool Clay order. The renderer lifts a defined color 32% toward white so charcoal and dark green remain visible on the black plate; object-row swatches show the source color with a 1px Hairline border.
- **Persistence:** the extruder assignment is kept on `SceneObject`; the 3MF writer puts it back into `model_settings.config` and the source project settings are copied, so repaired/recovered packages keep their colors.
- **Limit:** color is per object, not per-triangle or per-part within one object.

### Object Rows
- **Shape:** 54px, 1px bottom hairline; 14px visibility square at the left.
- **State:** filled square means visible; outline means hidden. Selected rows use Ash plus a 1px Electric Blue bottom rule. Clean objects read CLOSED in Graphite; defects read in Signal Red Text.
- **Plates:** a plate group header is a 90×30 outlined button; the active plate fills Blue Dim with an Electric Blue border and white label.

### Index Rail
- **Shape:** 76px wide, six 58px ticks; a 13px number and 10px tracked label; a 14px hairline tick at the left, 30px Electric Blue when active, and a 1px blue bottom rule. The sixth tick is `ORIENT`.
- **Interaction:** clicking a tick scrolls the inspector to that section and reopens a hidden inspector. When a phase has no model, object or file sections (failed or loading), those ticks are dimmed and non-interactive; the failed phase labels the fifth tick DETAIL.

### Section Heads
- **Style:** `0N / NAME` in 11.5px tracked Graphite caps with an optional right-aligned meta, followed by a 1px Hairline.

### Help Overlay
- **Style:** 480px opaque black sheet, 1px Hairline Bright border, 28px inner margin, no title bar, no shadow, no fade. The heading is White, never Electric Blue; rows align action (Graphite) against keys (White) at 22px pitch on tall windows and 20px below 760px, so the enlarged 18-row list fits 1024×700 and 1024×680. The blue is reserved for the border, focus and close action. The frame is dimmed behind it; the rows include Settings, Send to slicer and Repair.

### Empty / Error / Loading
- **Empty:** the 76px wordmark, a 120px Electric Blue rule, a 27px headline, 15.5px body copy, a filled blue primary and an outlined blue secondary; the dashed 300×210 drop target uses a 1px Electric Blue border at ≥900px viewport width.
- **Error:** the same plate frame; a 30px Signal Red title, the plain error message at 14.5px, a filled blue primary recovery and an outlined secondary. When the failure is a recoverable external-part 3MF, the title becomes `PACKAGE NEEDS RECOVERY` in Amber, the body names the external parts, and the primary is `FIX ON LOAD` with an outlined `OPEN ANOTHER`.
- **Loading:** the file name at 31px, a Graphite parsing line, and an indeterminate 2px Electric Blue sweep on a Hairline track; the inspector's READING verdict stays white, and the blue is the live sweep only.

### Signature Component: Defect Callout
A 28px black-filled circle with a 1px defect-ink stroke, a centered 12px number, a leader to the exact defect centroid, and a 3.5px cross at the point. Clicking or Enter focuses the camera; hovering names the type and count. The marker color is the legend color — the mesh, legend and rail share one vocabulary.

### Defect Theater
- **Entry:** clicking a numbered callout or pressing `D` enters theater on the focused defect. The camera flies outward to the defect's side on a 0.62s ease-out dolly, the focus reveals over 0.30s, and `D` moves to the next defect.
- **Field:** the specimen becomes a near-black ghost; the grid, selection box and all other callouts yield. Only the focused defect kind keeps its full ink; other defect kinds sit at 18% in the x-ray line pass. A `THEATER` tag replaces `GRID ON` in the tag row, the side report stays live, and the marker keeps a 1px Electric Blue focus ring.
- **Focus frame:** a 1px White reticle with 8px corner ticks traces the projected local defect bounds. It has no fill, no shadow and no glow; the defect ink stays inside it. The frame is UI, not geometry.
- **Exit:** `Esc` returns on a 0.55s dolly and fades the ghost over 0.22s. Manual orbit, pan or zoom cancels the flight but keeps theater for inspection; `F`, a view preset, a new file or a repair ends theater outright. There is no idle animation.
- **Motion:** one authored camera move; no particles, bloom, chromatic aberration or looping pulse.

### Topology Loupe
- **Shape:** a 260×184 black plate at wide viewports and 208×154 at 1024; 1px Hairline border, 0px corners, 12px map inset, defect-ink corner ticks, no shadow or glass. It sits above the defect legend at bottom-right and connects to the white focus frame with a 1px White leader.
- **Header:** `LOUPE / <KIND>` in 8px tracked White; a right-aligned `× N` in the kind ink.
- **Map:** local defect segments in the kind ink, fitted into the plate. Open-edge endpoints carry valence labels only where the local valence is 1 or 3+; winding segments carry a two-stroke arrow; non-manifold segments carry a knot dot; degenerate faces draw a flattened violet triangle and a cross. A center crosshair fixes the marker.
- **Footer:** `LOCAL n / N` at 1024, expanded to `<DETAIL> · LOCAL n / N` at 1440; `ESC EXIT` / `ESC EXIT · D NEXT` on the right. The map is a reading of the existing defect lines, never new geometry.
- **Safety:** the loupe shows topology, not repair; it never labels a model watertight.

**The One Story Rule.** Theater holds one defect kind and one marker at a time. The focused kind keeps its ink at full strength; everything else becomes the ghost. Competing colored stories are forbidden.

**The Frame Is UI Rule.** The focus frame is 1px White with corner ticks; defect ink belongs to the geometry, and Electric Blue belongs to the interactive marker. Never paint a colored disc or halo around the focus.

**The No Aura Rule.** No bloom, glow, translucent disc, gradient or looping pulse. The ghost, the white frame, the ink lines and the topology loupe carry the moment.

## Do's and Don'ts

### Do:
- **Do** keep every background at `#000000`; use 1px hairlines and blue borders for separation.
- **Do** keep Electric Blue for interactive state only: buttons, borders, focus, active segments and selection.
- **Do** keep the defect inks semantic on the mesh, in the legend and in the callouts; use the darker line variants on the lit mesh.
- **Do** keep content text at Graphite or brighter; Graphite Faint is decoration only.
- **Do** keep the larger ramp: 10px minimum for rail labels, 12.5px explanations, 15px rows, 18px data, 44px verdict.
- **Do** keep 0px corners, 1px rules and no shadows.
- **Do** follow BambuStudio for the viewport's hands and the Ctrl/Cmd+1–7 view presets.
- **Do** show real parse and check timings; label synthetic samples and the STL millimetre assumption.
- **Do** use opaque black sheets with a 1px Hairline Bright border and 0px corners; they are plates, not cards.
- **Do** mark `REPAIRED PREVIEW` while previewing and `REPAIRED` only after Keep; disable Keep when no safe pass changed geometry.
- **Do** state what repair did not heal; non-manifold edges are reported, never claimed fixed.
- **Do** ghost the specimen, not the defect; keep the focused kind in its ink and the rest of the field quiet.
- **Do** use the White focus frame and the Blue selected-marker ring; no colored disc, halo or bloom.
- **Do** keep the side report live in theater and return the camera on `Esc`.
- **Do** stream large binary STL as a live flat mesh and let the six-cell filmstrip carry the analysis progress.
- **Do** frame orientation as a preview on an assumed 256³ bed; rotate the specimen, never the source.
- **Do** offer `FIX ON LOAD` when a large 3MF fails on external object parts, and keep every plate and object name after recovery.

### Don't:
- **Don't** add eyebrow or kicker labels above headings; section locators live in the rail and the section heads.
- **Don't** use a tonal panel fill to separate chrome from the viewport.
- **Don't** use Electric Blue as a defect ink or as a decorative accent on data.
- **Don't** use Graphite Faint for body, explanations, metas or state values.
- **Don't** introduce cards, shadows, glass, gradients, pills or rounded corners.
- **Don't** show idle FPS or invented performance numbers.
- **Don't** use warm clay or sodium/warm-yellow accents; the surface is black, white and cool blue.
- **Don't** put a second status stack beside the calibrated rail.
- **Don't** claim non-manifold healing or self-intersection detection; neither is in the safe repair.
- **Don't** overwrite the source file when saving or sending a repair.
- **Don't** add particles, bloom, glow, chromatic aberration or idle loops to theater.
- **Don't** let two defect kinds compete for the theater's ink at full strength.
- **Don't** show a spinner for a large model when a streamed mesh can be visible.
- **Don't** imply the orientation adviser changed the file or found a new defect fact.
- **Don't** imply package recovery repaired the mesh; the checks and Repair flow still own that.
