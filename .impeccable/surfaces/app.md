---
version: 1
slug: "app"
primary_target: "app"
related_targets: []
---

# Surface: ModelView desktop inspector

## Scope and mode

The whole desktop application: first launch, empty state, progressive large-STL load with analysis filmstrip, file loaded, analysis result, error, collapsed inspector, help overlay, slicer settings sheet, repair sheet with live preview, defect theater with its topology loupe, and the orientation adviser. Mode: Operate. The visitor is a maker checking a model before printing; success is a trustworthy verdict in seconds with the defect visible on the mesh. The user has since pinned a cleaner surface: pure black, white type, electric blue interaction, and a larger type ramp for high-resolution displays.

## Audience and job

Makers and hobbyists at a desk, file just downloaded, slicer waiting. They need to know: is it watertight, how big, how many triangles, and where is the problem. They already know BambuStudio's viewport, so orbit/pan/zoom must match it without a tutorial. The app is read on high-DPI laptop and desktop displays at a comfortable distance; the smallest content labels must stay near 10–11px, details at 12.5px, body at 14.5–15px, data at 18px.

## Content and constraints

Real geometry facts only: triangle/vertex counts, bounding box, volume, surface area, shells, open edges, non-manifold edges and vertices, degenerate faces. No invented scores, print-time estimates, or material claims. Slicer detection may read declared install locations; repair runs locally on a worker and writes only a user-chosen or OS-temp copy. Windows and macOS first, Linux optional. One process, wgpu, no webview. The surface is pure black with no tonal panel fills; separation comes from 1px hairlines and blue interactive borders. The object mesh is cool white/blue-gray; the defect inks are semantic. Settings and Repair are opaque Blueprint plates with the same hairlines and 0px corners; repair never overwrites the source. Defect Theater ghosts the specimen, keeps one defect kind in its ink, frames it in 1px White, and reads the same defect lines in a topology loupe; it adds no checks or geometry. Large binary STL streams a live flat mesh before the authoritative checks land; the analysis filmstrip shows provisional counts. Orientation Adviser previews candidate poses on an assumed 256³ bed and never writes the source. Large 3MFs that keep geometry in external production parts offer a `FIX ON LOAD` recovery that preserves plates and object names, skips broken parts and opens the scene in memory. Defined Bambu 3MF part colors are matched through model/project settings and shown on the mesh and in object-row swatches; undefined parts use the Clay palette.

## Chosen direction and memorable moment

Blueprint, chosen from the critique's three cleaner options: black field, white specimen, electric blue line, four defect inks, Archivo Narrow with a larger ramp. The memorable moment: the electric blue line connects the calibrated rail, the active control, and the selected object, while the defect inks stay independent so a red open edge never reads as a generic accent. Defect Theater turns that vocabulary into motion: the camera flies to the defect, the plate goes dark, the focused ink and a White focus frame hold, and a topology loupe docks bottom-right. Standard desktop controls throughout; the world supplies type, palette, density, and that blue line — never the navigation model. The inspector is hidden with I and reopens from the rail.

## Unresolved decisions

None blocking. Self-intersection and wall-thickness checks remain out of scope for v1. Repair splits non-manifold joins only when the AFTER report improves without opening new edges; remaining joins are reported, never claimed fixed. Deep remesh is an opt-in SDF rebuild that changes detail; it is labelled as a rebuild, never as a safe pass.

## Direction contract

THESIS: A pure black inspection plate with a white specimen, one electric blue interactive line, and four defect inks. It refuses the dashboard-of-cards arrangement every viewer in this category ships.

OWN-WORLD: A true `#000000` field for viewport, rail, inspector, top strip, and status bar, with white `#FFFFFF` primary text, `#A7B3BF` secondary, `#5C6B78` decorative only, `#161E28` structural hairlines, `#263545` interactive borders, and `#2F6BFF` as the single live accent. Defect inks: open edge `#FF5A5F`, non-manifold `#F5A524`, flipped winding `#14B8A6`, degenerate `#A78BFA`, with darker one-step variants for hairlines drawn on the lit mesh. Archivo Narrow: 76px wordmark, 44px verdict, 18px data, 15px row labels, 12.5px explanations, 11.5px metas, 10px rail labels; tabular-aligned measurements. Square, flat, no shadows, no tonal panels; states by line weight, border, and blue. Density is high; legibility is higher.

STORY: The maker opens an STL or 3MF, sees the white specimen on pure black before reading anything, reads one verdict and a short failure list, sees each failure marked in its defect ink, then opens the next file. The report never hides the object.

FIRST VIEWPORT: 1440×900. A 66px top strip carries the wordmark, the file and its format, four camera presets, and one Open action. A 76px index rail with six 58px ticks runs down the left, ending in the Settings gear and Help `?`. The central plate holds the shaded cool-white mesh with numbered defect markers and a bottom defect legend; the scale bar sits under the axis gizmo at top right. The 440px right panel runs a vertical hairline rail through a 44px verdict, six 46px checks, an 18px data plate, and object rows; a loaded verdict then carries Repair/Send or a single Send. 1024×700 keeps the same chrome with a 508px viewport and a single-column empty state. A 38px status bar closes the frame. Settings centers a 540px black sheet over a dimmed frame; Repair centers the same plate over dimmed chrome while keeping the live preview and status readout lit, and adds `REPAIRED PREVIEW`, then `REPAIRED` after Keep. When a sheet would clip the fixed topbar, status bar or inspector at narrow/short sizes, that chrome yields blank for the sheet's lifetime; panel widths, the rail and the preview stay put. Defect Theater (D or a callout click) replaces the grid tag with `THEATER`, ghosts the specimen, frames the focused defect in 1px White, and docks a 260×184 topology loupe above the legend; the side report stays, and Esc returns the camera. At 1024 the loupe is 208×154 and the footer shortens. Large binary STL streams a flat specimen under `LARGE MODEL / STREAMING`, with a six-cell analysis filmstrip above the bottom edge; `ORIENT` opens a side-section adviser with `FITS BED`, candidate poses and a GPU bed grid at z=0. The rail has six ticks. A recoverable 3MF failure replaces the generic error body with an amber `PACKAGE NEEDS RECOVERY` offer and a filled `FIX ON LOAD`; the recovery state says `RECOVERING / RESOLVING EXTERNAL OBJECT PARTS`, and the loaded scene carries `RECOVERED`.

FORM: User-directed replacement after the critique's cleaner options: Blueprint + Large +3px. The predecessor Emission Rail surface is retired. No roll or seed applies; the user pinned pure black, white, electric blue, and larger type, and chose this option from three. seed: n/a

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance.
