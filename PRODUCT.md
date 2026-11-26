# Product

<!-- impeccable:product-schema 1 -->

## Platform

desktop

## Stack

Rust. Native window and chrome from eframe/egui, dressed in shadcn's token system; the 3D viewport is a custom wgpu surface in the same process. Chosen by the user for single-process GPU-native rendering, one binary per OS, and no webview in the frame path.

## Users

Makers and hobbyists, at a desk, on a laptop, often with a slicer open in another window. They download an STL or 3MF from MakerWorld, Printables, Thingiverse, or a chat thread and want to know, before committing a print, whether the file is sound: is it watertight, how big is it, how heavy is the triangulation, will it slice? They already use BambuStudio, so its viewport is muscle memory, not a tutorial.

## Product Purpose

Open an STL or 3MF, see it immediately, and know whether it is printable. Success is a trustworthy verdict within a second of the drop, with the exact numbers one hover away and no project setup, no slicing, no waiting for a full studio to boot.

## Positioning

A focused printability inspector, not a slicer. It opens the same files BambuStudio opens, moves the same way BambuStudio moves, and answers the questions BambuStudio buries behind project setup faster than BambuStudio can launch. A neighboring slicer could copy any single feature; it cannot copy the discipline of refusing to become one.

## Operating Context

Files arrive from model sites, chat, or a colleague's export. The user is mid-task and impatient; the inspection is a gate before slicing, not a design session. Models range from a 20 mm trinket to a multi-object 3MF project with plates and transforms. The machine is usually a MacBook or a Windows desktop; Linux is a bonus, not a promise.

## Capabilities and Constraints

- Read binary and ASCII STL.
- Read 3MF at full project semantics: object meshes, build items, transforms, plates, and project/settings metadata where present.
- Printability tests, primarily a watertight/manifold check with the defect location surfaced in the viewport.
- Model facts: triangle and vertex counts, bounding box and dimensions, volume, surface area, object count, open-edge count.
- GPU rendering through wgpu; multi-million-triangle meshes must orbit smoothly.
- Viewport navigation matches BambuStudio: left-drag orbit, right-drag pan, wheel zoom toward cursor, standard view hotkeys.
- Slicer handoff: detects Bambu Studio, OrcaSlicer, PrusaSlicer, UltiMaker Cura and a custom app/executable, remembers the choice, and sends the original, the saved repaired copy, or a temporary copy.
- Safe repair: previewable weld, degenerate/duplicate removal, winding fix and boundary-hole fill for non-watertight scenes; Keep, Revert, save a repaired STL/3MF, or send it. The source is never overwritten.
- Defect Theater: `D` or a numbered callout flies the camera to the defect, ghosts the specimen, keeps one defect kind in its ink inside a White focus frame, and docks a topology loupe with local valence, winding arrows and non-manifold knots; `Esc` returns the camera.
- Defect Theater is a reading of existing analysis, not a new check: the side report stays live and no new printability claims are introduced.
- No-wait large models: binary STL above a threshold streams a live flat-shaded mesh first, with an analysis filmstrip counting provisional open/manifold/winding/volume/shells/degenerate results; the final smooth scene replaces it when the authoritative report lands.
- Orientation Adviser: on an assumed 256 × 256 × 256 mm bed, test candidate poses, rank lowest profile, support risk and bed contact, and animate the specimen onto the recommended pose. Preview only; the source file is never rewritten.
- Large 3MF recovery: if a package cannot open because its objects live in external production parts, offer **FIX ON LOAD**, resolve the parts, preserve plates and object names, skip broken parts, and open the recovered scene in memory. The source is never rewritten.
- Defined 3MF part colors: Bambu model-settings extruder assignments are matched to project filament colors and shown on the mesh and object-row swatches; undefined parts keep the cool specimen palette. Dark print colors are lifted for the black plate. Repaired/recovered 3MF exports retain the extruder assignment.
- Repair scope is explicit: non-manifold edges stay reported and unresolved; self-intersections are not detected. The app never claims either was fixed.
- Targets Windows and macOS; Linux is optional and must not compromise the other two.

## Brand Commitments

The user pinned the UI language twice: first to shadcn's token system (neutral surface scale, consistent radius, visible focus ring, muted secondary surfaces), then to a cleaner Blueprint surface — pure `#000000` backgrounds, white primary text, one electric blue for buttons, borders, focus, and selection, and a larger type ramp for high-resolution displays. This is a visual constraint to honor, not a stack choice; it does not license React or a webview. The earlier charcoal/bone/sodium Emission Rail surface is retired.

## Evidence on Hand

Greenfield; no sample assets or fixtures yet. Test material may be fetched from public model sources and must be labeled with its origin.

## Product Principles

1. Answer the printability question first; statistics support the verdict, never replace it.
2. Speed is the feature. Every frame and every file open has a performance budget.
3. Borrow the user's existing muscle memory instead of teaching new controls.
4. Be explicit about what changed: repair is previewed, Keep is a deliberate act, and the after report shows what remains. Orientation is a preview on an assumed bed, never a silent file rewrite.
5. Keep the viewport pure; chrome recedes until it is needed.
6. Translate geometry truth into plain language, with exact numbers one hover away.

## Accessibility & Inclusion

Every action is keyboard-reachable: open, view presets, panel toggles. The watertight verdict never relies on color alone; it carries a label and an icon.
