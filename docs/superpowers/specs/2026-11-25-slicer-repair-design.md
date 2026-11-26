# ModelView — slicer handoff and safe repair

Status: **Proposed — awaiting approval**
Date: 2026-11-25
Surface: ModelView desktop app (Operate). Existing Blueprint design system.

## Intent

Close the loop between finding a model and printing it, without turning ModelView into a slicer:

1. Let the maker choose and save the slicer installed on this machine, then send the current model straight to it.
2. Let the maker repair the common topology errors ModelView reports, preview the result, save a repaired copy, and send that copy to the slicer.

The app remains an inspector and repair bench. It never slices, generates G-code, changes print settings, or silently alters a file on disk.

## Decisions already made

- The user chose **safe repair + 3MF preservation**:
  - weld coincident vertices
  - remove invalid/degenerate faces
  - remove exact duplicate faces
  - fix winding/orientation
  - fill boundary holes
  - preview before applying, save a new STL or 3MF, and report unresolved non-manifold edges instead of pretending to heal them. Self-intersections are not analyzed in v1 and are never claimed fixed.
- The slicer setting is auto-detected from common installs plus a custom executable, saved across restarts.
- The handoff uses the original file when untouched, or a repaired copy when repaired.
- The interface stays in the existing Blueprint system: pure black, white type, Electric Blue interaction, defect inks, 0px corners, no shadows.
- macOS and Windows are first-class; Linux builds from source and remains optional.

## Goals

- Persist the chosen slicer and restore it on the next launch.
- Detect BambuStudio, OrcaSlicer, PrusaSlicer and UltiMaker Cura on macOS/Windows, plus a custom executable.
- A clear **Send to <slicer>** action that launches the current file in the chosen slicer and reports failure plainly.
- A **Repair…** action on a non-watertight report (including Warning states) that opens a repair sheet with before/after counts, selectable safe fixes, and a live viewport preview.
- A repaired copy writer that preserves a 3MF scene's objects, names and plate assignments, and copies the original Bambu-style project settings where present.
- Non-destructive by default: the original file is never overwritten unless the user explicitly chooses its path in the save dialog, and the default save is a new `*.repaired.*` copy.
- Keyboard, focus and 1024/1440 behavior consistent with the existing app.

## Non-goals

- Slicing, G-code, print-time or material estimates.
- Editing printer/filament settings or plate layout inside ModelView.
- Automatic healing of non-manifold edges in v1. Non-manifold edges remain in the report as unresolved. Self-intersection detection is also out of scope: the analyzer has no such check and this feature does not add one.
- A full CAD/mesh-editing suite, interactive hole picking, sculpting, or boolean operations.
- Cloud upload, accounts, or remote repair.
- Bundling or downloading a third-party repair binary.
- Changing the existing STL/3MF parser, analysis scores, BambuStudio navigation or the Blueprint visual system.

## Anti-goals

- Silently reporting a repaired mesh as watertight when it is not.
- Claiming a self-intersection was found or healed; the app does not analyze self-intersections.
- Making the slicer handoff depend on a specific slicer's undocumented command line.
- Modifying the original file in place without an explicit save decision.
- Turning a repair failure into a crash or a stuck modal.
- Replacing the calibrated rail/viewport-first report with an editor layout.
- Shipping a feature that only works on the developer's machine because of hard-coded user paths.

## Constraints

- Rust workspace:
  - `modelview-core`: formats, mesh, analysis, repair, export. No UI.
  - `modelview-app`: eframe 0.37 / egui 0.37 / wgpu 30, Blueprint theme, sheets, persistence, slicer process launch.
- Existing crates only, plus eframe's `persistence` feature and `serde` (already in the workspace through eframe/zip dependencies; add explicitly as needed). No new mesh-repair binary or service.
- `rfd` for file open/save and executable selection.
- `std::process::Command` for launch; no shell, no string interpolation into a shell.
- Write tests for core topology, export round-trips, detection/launch-spec construction, and settings serde.
- The repair must run without blocking the UI on large meshes.

## Approaches considered

### A. Self-contained core-first repair + export + persisted settings (chosen)

- Repair topology in `modelview-core`, write STL/3MF there, persist settings through eframe, detect and launch the slicer in the app.
- Own the quality, tests and preview. No external tool dependency. Fits the local, single-process product.
- Cost: we own hole filling, winding repair and a 3MF writer.

### B. Let the slicer repair the model

- Shell out to a slicer's repair/CLI or a bundled mesh-fixer (admesh/MeshLab) and launch the result.
- Rejected: the capable repair CLIs are undocumented or platform-specific, the preview is lost, and the app would depend on silent external behavior. It also blurs the product boundary ("we repaired" vs "the slicer did").

### C. Third-party mesh-repair library

- Integrate a robust repair crate or service.
- Rejected for v1: no clearly maintained Rust library covers the safe set plus 3MF, the licensing/quality surface is uncertain, and the safe operations are small enough to own and test.

### D. Full healing (non-manifold + self-intersection)

- Attempt edge splitting, self-intersection resolution and robust remeshing.
- Deferred deliberately. It is a much larger, less predictable geometry problem and can silently change the model. The user chose the safe path. The report will say what remains.

### E. One-click repair vs preview

- One-click is smaller but hides what changes.
- Preview is chosen: repair changes geometry and the sample hole is visible; the user must see the before/after and be able to revert.

## Design

### 1. Settings and persistence

Add `Settings` to the app:

```rust
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    pub slicer: Option<SlicerChoice>,
    pub last_open_dir: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SlicerChoice {
    pub name: String,        // "Bambu Studio", "OrcaSlicer", …
    pub kind: SlicerKind,    // Bambu, Orca, Prusa, Cura, Custom
    pub path: PathBuf,       // /Applications/BambuStudio.app or an executable
}
```

- Enable eframe's `persistence` feature; load with `eframe::get_value(cc.storage, eframe::APP_KEY)` in `ModelView::new`, save in `App::save`.
- The settings panel updates `self.settings`; closing the panel/exit persists. A small "Saved" line confirms the choice without a modal toast.
- `last_open_dir` replaces the current in-memory `last_dir` so Open remembers the folder.
- If no storage is available (unusual), settings remain in memory and the panel says "Not saved for next launch" once.

### 2. Slicer detection and launch

`crates/modelview-app/src/slicer.rs`:

```rust
pub enum SlicerKind { Bambu, Orca, Prusa, Cura, Custom }
pub struct SlicerCandidate { pub name: String, pub kind: SlicerKind, pub path: PathBuf }

pub fn detect_slicers() -> Vec<SlicerCandidate>;
pub fn launch_command(choice: &SlicerChoice, file: &Path) -> std::io::Result<std::process::Child>;
```

Detection (best effort, never blocking):
- macOS: `/Applications` and `~/Applications` app bundles:
  - `BambuStudio.app`, `OrcaSlicer.app`, `PrusaSlicer.app`, `UltiMaker Cura.app`.
- Windows: scan `%ProgramFiles%`, `%ProgramFiles(x86)%`, `%LOCALAPPDATA%` for known executable names under common vendor folders (Bambu Studio, OrcaSlicer, Prusa3D/PrusaSlicer, UltiMaker Cura version folders).
- Linux: `PATH` entries for `bambu-studio`, `orca-slicer`, `prusa-slicer`, `cura`; a custom AppImage/binary remains available.
- The user can always Browse to an `.app`/`.exe`/binary. The custom candidate is named from the file stem and marked Custom.

Launch:
- macOS:
  - If the path is a directory ending `.app`: `/usr/bin/open -a <app> <file>`.
  - Else: spawn the executable with the file as its single argument.
- Windows/Linux: spawn the executable with the file as its single argument.
- Spawn detached; do not wait; report spawn errors (missing path, permission, missing app) inline in the sheet/status.
- Security: only the user-selected/declared path is executed; file paths are arguments, never shell strings.

### 3. Send to slicer and working-file policy

- `Send to <name>` lives in the verdict block when a model is loaded:
  - If a slicer is configured, it is the primary filled action when the verdict passes, and an outlined action beside **Repair…** when it fails.
  - If no slicer is configured, it reads **Set up slicer** and opens the settings sheet.
  - `Cmd/Ctrl+Enter` sends; the help sheet lists it.
- The file sent is:
  1. the original path when the scene has not been repaired and the file still exists;
  2. the saved repaired copy when one exists for the current repair result;
  3. otherwise a temporary repaired copy in the OS temp dir, written only for the launch. The status says "Opened a temporary repaired copy in <slicer>" and the repair sheet offers "Save repaired copy…" for permanence.
  4. if the built-in sample (no original path) is active, a temporary STL/3MF of the current scene is written; the app never needs the user to save first just to demo the handoff.
- If the original path has been moved/deleted, the app falls back to exporting the current scene to a temp file and continues; it never launches a missing path.
- The 3MF temp/export preserves objects/plates and copies the original `Metadata/project_settings.config` when the source is a 3MF on disk.

### 4. Safe repair engine (`modelview-core/src/repair.rs`)

```rust
#[derive(Clone, Copy, Debug)]
pub struct RepairOptions {
    pub weld: bool,
    pub remove_degenerate: bool,
    pub remove_duplicate_faces: bool,
    pub fix_winding: bool,
    pub fill_holes: bool,
}
impl Default for RepairOptions { /* all true */ }

#[derive(Clone, Debug, Default)]
pub struct RepairStats {
    pub welded_vertices: usize,
    pub degenerate_removed: usize,
    pub duplicates_removed: usize,
    pub faces_flipped: usize,
    pub holes_filled: usize,
    pub patch_faces: usize,
    pub unresolved_non_manifold: usize,
}

pub fn repair_mesh(mesh: &Mesh, options: RepairOptions) -> (Mesh, RepairStats);

/// Each scene object repaired independently; names, plates and source ids preserved.
pub fn repair_scene(scene: &Scene, options: RepairOptions) -> (Scene, Vec<RepairStats>);
```

Pipeline:

1. **Weld** coincident vertices when enabled, with the same relative tolerance as the analysis. STL always needs it; 3MF usually benefits and the operation is idempotent for already-shared meshes.
2. **Drop invalid and degenerate** triangles (`area <= 1e-9 mm²`), and count them.
3. **Drop exact duplicate faces** (same three vertex ids regardless of rotation; opposite winding duplicates are also removed, preferring the first).
4. **Fix winding**:
   - Build undirected edge incidence.
   - BFS/union-find connected face components; for every shared edge, flip a face when both faces traverse it in the same direction.
   - After a component is consistent, compute its signed volume. If negative, flip the whole component unless it is a cavity nested inside a positive shell; a cavity shell must keep its inward orientation or a hollow part would be silently turned into two outward solids. Containment is tested with a point of the negative component against the positive component's triangles (ray casting); with few components this is cheap. For open components the sign still indicates the intended outside; holes are filled after this pass.
5. **Fill boundary holes**:
   - Collect directed boundary edges (`count == 1`) after winding.
   - Walk them into closed loops.
   - For each loop with ≥3 distinct vertices, average the loop points into one new vertex and add a triangle fan using the opposite direction of each boundary directed edge, so the new faces agree with the existing surface.
   - Skip a loop whose fan would create a degenerate triangle; count it as unfilled and leave it for the report.
   - The fan is a repair patch, not an inferred original surface; the repair sheet says so in one line.
6. **Tidy**: remove unreferenced vertices; re-analyze with the existing analysis (the source of truth for before/after).
7. **Do not attempt** non-manifold edge splitting, self-intersection resolution, voxel remeshing or decimation. Non-manifold leftovers remain in the post-repair report and are stated as unresolved.

The post-repair verdict uses the existing `analyze` / `summarize`; no shortcut that trusts the repair.

Per-object behavior:
- Multi-object 3MF: every object is repaired independently; the scene keeps names, plate ids and source ids.
- If any object cannot become watertight, the scene verdict remains failed; the sheet's after summary and the object rows say which.
- The original scene is kept in the repair session until the sheet is applied or closed; the original file is untouched.

### 5. Repaired STL and 3MF writer (`modelview-core/src/export.rs`)

```rust
pub fn write_stl(path: &Path, mesh: &Mesh) -> Result<()>;
pub fn write_3mf(
    path: &Path,
    scene: &Scene,
    project_settings: Option<&str>,
) -> Result<()>;
```

STL:
- Binary STL, one normal per face; positions and normals in millimetres.
- Round-trip test with the existing parser.

3MF:
- A valid core 3MF zip with `[Content_Types].xml`, `_rels/.rels`, `3D/3dmodel.model`.
- One object per scene object, object ids 1..N, names preserved and XML-escaped, meshes in millimetres, build items at identity (the scene is already baked into world space).
- When the scene has plates: write `Metadata/model_settings.config` with the new object ids mapped to the original plate ids/names, so BambuStudio/Orca see the same plates.
- When the original source is a 3MF and its path still exists: copy its `Metadata/project_settings.config` verbatim so the printer/filament context survives. Treat it as opaque printer/filament context: Bambu-style project settings do not reference object ids. Do not copy the old `model_settings.config` because its object ids would be wrong; generate a new one. If a real project_settings file is found to carry object/plate references, fall back to a minimal project settings built from `Scene.project` rather than shipping stale ids.
- Re-parse the written file with `modelview_core::threemf::parse`; assert object count, names, plates and watertightness in tests.
- No thumbnails or unrelated metadata are copied.

### 6. UI: Settings sheet

Entry points:
- A gear button in the index rail's bottom control pair (beside the existing `?` help), plus `Cmd/Ctrl+,`.
- A **Set up slicer** action in the verdict block when no slicer is configured.

Sheet (Blueprint, black, blue border, no shadow, no fade; ~560px):
- Header `SETTINGS / SLICER`.
- One short line: "ModelView opens the current model in the slicer you choose. Files stay on this machine."
- Detected rows: name, path, a square radio; the selected row gets an Electric Blue border/fill; a `FOUND` tag or `MISSING` if the saved path vanished.
- `Browse…` for a custom `.app`/executable; the custom row shows the selected name/path.
- `Rescan` reruns detection; the saved selection survives if still present.
- A `SAVED` line or a "Will save when you close" line; no modal toast. The selection updates immediately in memory; eframe persists on close/auto-save.
- Close button; `Esc` closes.

### 7. UI: Repair sheet

Entry point:
- **Repair…** whenever the scene is Ready and not Watertight (open edges, non-manifold, winding, degenerate, or inward normals), including the Warning states, not only the red verdict. The sheet attempts the safe fixes and says "No safe repair applied" if nothing changes. `R` works for any non-watertight ready scene. The Watertight pass state has no repair action.
- The action opens the repair session and starts the default repair job.

Sheet (Blueprint, ~560px) while the repaired preview is active:
- Header `REPAIR / <file>`.
- `BEFORE` summary: open edges, non-manifold, winding, degenerate, shells (the same terms as the report).
- `AFTER` summary from the preview's analysis; a still-failing result is allowed and shown in Signal Red.
- Operation rows, all safe defaults on:
  - Weld coincident vertices — n
  - Remove degenerate faces — n
  - Remove duplicate faces — n
  - Fix winding — n
  - Fill boundary holes — n loops / n patches
  Each row has a square checkbox in the Blueprint style; changing a row starts a new preview job. The option rows are disabled while that job runs; there is no pending-option queue. The rows and counts re-enable when the new preview arrives.
- One disclaimer line: "Boundary holes are patched with a simple fan. Non-manifold edges are reported and stay unresolved; self-intersections are not checked."
- Actions:
  - **Keep repaired** (filled blue) — closes the sheet and leaves the preview active; the scene is marked `REPAIRED`.
  - **Revert** (outlined blue) — restores the original scene and closes the preview.
  - **Save repaired copy…** (outlined blue) — rfd save dialog; default `<stem>.repaired.stl` / `.3mf`; source format; writes the current preview; shows the saved path.
  - **Send to <slicer>** (filled blue when a slicer is configured, otherwise Set up slicer) — launches the repaired copy (saved or temporary).
- The viewport shows the repaired mesh behind the sheet, with a `REPAIRED PREVIEW` tag; the side panel report switches to the repaired report; the original scene and report are retained for Revert.
- Closing the sheet with the X/Esc is Revert. Applying is explicit.

Repair job behavior:
- Runs on a worker thread (the scene is owned by the job while running; the last GPU frame and the last report stay visible with a `REPAIRING` state on the sheet). On completion the app swaps in the preview and keeps the original for Revert.
- Large meshes do not freeze the frame loop; the sheet shows a 2px blue sweep while working. Option rows are disabled until the job finishes; a change after it finishes starts the next preview job.
- Errors are shown in the sheet, not as a crash; the original is restored.

### 8. State and data flow

Add to the app:
- `settings: Settings`, `settings_open: bool`
- `repair: Option<RepairSession>`
- `RepairSession { original: Scene, original_reports, original_summary, original_markers, preview: Scene, preview_reports, preview_summary, preview_markers, options, saved_path, applied }`
- `repair_job: Option<Receiver<RepairResult>>`
- `notice: Option<(String, Instant)>` for launch/save feedback in the status bar
- `slicer_candidates: Vec<SlicerCandidate>` detected at startup and on Rescan.

The existing scene/report/summary/markers/GPU state remain the display source; the repair preview replaces them while the session is active, and Revert moves the original back. The GPU is rebuilt through one scene-install helper used by loading and repair, instead of duplicating upload logic.

### 9. Keyboard, focus and accessibility

- `Cmd/Ctrl+,` opens Settings.
- `Cmd/Ctrl+Enter` sends to slicer.
- `R` opens Repair when the model is Ready and not Watertight and focus is not in a control.
- `Esc` closes Settings or the Repair sheet; in Repair, Esc reverts.
- The rail gear and new sheet buttons get the same 1px blue focus ring and Enter/Space activation as the existing controls.
- Every new radio/checkbox/button is reachable by Tab; the sheet has a visible focus order and a close action.
- The repair sheet's before/after is text plus labels, not color alone.

### 10. Errors and edge cases

- No slicer configured: Send opens Settings; the button reads "Set up slicer".
- Slicer path missing/invalid: Settings marks it missing; Send shows the spawn error in the status/notice and returns to the app without changing the scene.
- Original file missing after load: Send exports the current scene to a temp copy; no launch of a missing path.
- Unrepaired sample: Send writes a temp copy of the sample; no file dialog required.
- Save dialog cancelled: no file, no error; the sheet remains.
- Write failure (permission/disk/full): inline message; the scene and preview remain.
- Self-intersections: the analyzer does not detect them, so the app neither reports nor repairs them; the repair copy says this explicitly.
- 3MF with no plates: writer omits model_settings; valid core 3MF.
- 3MF with plates: preserve plate ids/names and object assignment; copy project settings when available.
- Non-manifold leftovers: the after summary shows them; Send remains available with the unresolved count, because the slicer may still accept the file. The app never calls the mesh watertight. Self-intersections are not checked, so the app makes no claim about them either way.
- A repair that does nothing: the sheet says "No safe repair applied" and the original is restored on Revert.
- Large mesh: worker; the app remains responsive; the preview appears when ready.
- macOS/Windows path spaces and Unicode: `PathBuf` and `Command` args; no shell.
- Linux optional: detection is best effort; Browse and launch still work.

### 11. Performance

- Repair and analysis run in a worker thread; the viewport and sheet stay live.
- Hole loops and edge maps are linear in triangles; no O(n²) over the mesh.
- The 3MF writer streams one object at a time; the repaired file is only written on save/send, not on every preview.
- The preview reuses the existing GPU upload path; no extra copies beyond the repaired mesh.

### 12. Tests

Core (`modelview-core`):
- `repair`:
  - nested outer box plus inner cavity (two shells, opposite orientation) keeps the cavity inward; the signed volume is the material volume, not the sum of two solids.
  - tetra with a missing face → fill → watertight, 0 open edges, volume preserved within the patch tolerance.
  - tetra with one flipped face → fix winding → watertight, 0 inconsistent edges.
  - degenerate sliver removed; count and watertightness.
  - duplicate face removed; a non-manifold edge caused by duplicates becomes clean where possible.
  - two-shell scene repaired independently; shells preserved.
  - open box with a hole → fill; non-manifold case remains reported.
  - repair stats match the before/after analysis.
- `export`:
  - STL binary round-trip through `stl::parse`; watertight; triangle count.
  - 3MF round-trip through `threemf::parse`; object names, plates, unit mm, triangle counts.
  - 3MF with a single object, a multi-object project, and no plates.
  - project settings copy when provided.
- Existing analysis tests remain green; the sample's 24 open/3 flipped defects repair to watertight.

App (unit-level, no GPU):
- `Settings` serde round-trip.
- Slicer detection against a temporary root with fake app/exe names; no real system paths.
- `launch_command` constructs the expected macOS `.app`/executable and Windows executable arguments without spawning.
- Working-file policy: original path, repaired temp, sample temp, missing original.
- Repair option change produces a new preview and Revert restores the original scene.

Manual/visual QA:
- Settings sheet at 1440 and 1024; detected, custom, missing, saved.
- Repair sheet on the broken sample: before 24 open/3 flipped, after 0/0; preview; revert; keep; save STL; send.
- A 3MF project with a broken object: repair one object, save a repaired 3MF, re-open in ModelView and in the slicer; plates preserved.
- Send to a real slicer on macOS with BambuStudio or Orca; original and repaired paths.
- Error: invalid slicer path, cancelled save, missing original, read-only folder.
- Loading/repair worker keeps the frame loop responsive.

## Acceptance criteria

- Choose a slicer, restart ModelView, and the choice is still selected.
- `Send to <slicer>` opens the original file when untouched and the repaired copy after repair; failures are explained in place.
- The broken sample repairs to a watertight mesh in the preview, the original file is unchanged, and a repaired STL can be saved and sent.
- A broken multi-object 3MF repairs per object and saves a 3MF that re-parses with the same objects, names and plates. The writer is checked with the shipped project sample, and the repaired file is opened manually in a detected BambuStudio/Orca install for the final compatibility check. The unresolved non-manifold defects, if any, are named; self-intersections are not claimed.
- Non-manifold leftovers are reported; self-intersections are not checked or claimed.
- All new controls are keyboard reachable with the Blueprint focus ring; 1440 and 1024 layouts hold.
- `cargo test`, `cargo clippy --all-targets` and `cargo fmt --check` pass.

## Risks

- **Hole-fan quality**: a simple fan can patch a non-planar or branching hole imperfectly; the design limits the claim and the after-analysis shows the result.
- **3MF compatibility**: a rewritten 3MF loses the original object graph/thumbnails; it keeps objects, plates, names and project settings, which is what the slicer needs. Verify in BambuStudio/Orca. If a project_settings file carries object references, the writer falls back to a minimal project settings rather than copying stale ids.
- **Slicer detection variance**: Windows install layouts vary; the custom Browse is the guaranteed path.
- **Persistence timing**: eframe saves on exit/interval; a hard kill can lose the last selection. Acceptable; the sheet can state it.
- **Large-mesh preview cost**: worker + a copy of the original while previewing; memory/CPU bound; the design keeps the UI responsive and the operation explicit.
- **macOS launch permissions**: `open -a` for app bundles and executable spawn for custom paths; Gatekeeper may require the slicer to have been launched once.

## Files expected to change

- `crates/modelview-core/src/repair.rs` (new)
- `crates/modelview-core/src/export.rs` (new)
- `crates/modelview-core/src/lib.rs`
- `crates/modelview-app/src/slicer.rs` (new)
- `crates/modelview-app/src/settings.rs` (new, or app-local module)
- `crates/modelview-app/src/app.rs` (sheets, actions, state, scene install)
- `crates/modelview-app/src/theme.rs` (sheet/radio/checkbox tokens only)
- `crates/modelview-app/src/main.rs` (eframe persistence feature via Cargo; no new UI)
- `crates/modelview-app/Cargo.toml` (persistence, serde)
- `DESIGN.md`, `.impeccable/design.json` (settings/repair sheet components added to the Blueprint system)
- `README.md` (features, settings, repair limits, save/send behavior)
- `docs/superpowers/specs/` (this document)

## What is left to the builder

- Exact hole-loop walking and fan orientation edge cases; the tests define the contract.
- The precise Windows/macOS candidate paths and executable names; the custom Browse covers gaps.
- Whether to keep the original scene in memory for Revert during a long preview or to re-read it; the user-visible contract is Revert restores it.
- The visual detail of the two sheets within the Blueprint tokens (width, row heights, section order) as long as the existing components, 0px corners, blue interaction and focus rings hold.
- The eframe persistence API details for the pinned 0.37 version.
- The exact wording of the repair disclaimer and the temporary-copy notice.

## Approval

This document is the full design for a project-sized change. No product code is written until it is approved. After approval, the implementation goes through `writing-plans`, then TDD for the core repair/export, then the app sheets and slicer handoff, then the visual and finish checks.
