use eframe::egui;
use modelview_core::{
    DefectMarker, Format, MeshReport, RepairOptions, RepairStats, Scene, analyze, repair_scene,
    summarize,
};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};

pub struct RepairPreview {
    pub scene: Scene,
    pub reports: Vec<MeshReport>,
    pub summary: MeshReport,
    pub markers: Vec<(usize, DefectMarker)>,
    pub stats: Vec<RepairStats>,
}

pub struct RepairSession {
    pub original: Scene,
    pub original_reports: Vec<MeshReport>,
    pub original_summary: MeshReport,
    pub original_markers: Vec<(usize, DefectMarker)>,
    pub options: RepairOptions,
    pub revision: u64,
    pub saved_path: Option<PathBuf>,
    pub saved_revision: Option<u64>,
    pub preview_active: bool,
    pub stats: Vec<RepairStats>,
    pub original_repaired: bool,
    pub original_saved_path: Option<PathBuf>,
}

pub fn build_preview(scene: &Scene, options: RepairOptions) -> RepairPreview {
    let mut options = options;
    if scene.format == Format::Stl {
        options.weld = true;
    }
    let (repaired, stats) = repair_scene(scene, options);
    let weld = repaired.format == Format::Stl;
    let reports: Vec<MeshReport> = repaired
        .objects
        .iter()
        .map(|object| analyze(&object.mesh, weld))
        .collect();
    let summary = summarize(&reports);
    let markers = reports
        .iter()
        .enumerate()
        .flat_map(|(index, report)| report.markers.iter().map(move |marker| (index, *marker)))
        .collect();
    RepairPreview {
        scene: repaired,
        reports,
        summary,
        markers,
        stats,
    }
}

pub fn start_repair(
    scene: Scene,
    options: RepairOptions,
    ctx: egui::Context,
) -> Receiver<Result<RepairPreview, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(|| build_preview(&scene, options))
            .map_err(|_| "The repair worker failed.".to_string());
        let _ = tx.send(result);
        ctx.request_repaint();
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;
    use modelview_core::{LengthUnit, SceneObject};

    fn broken_stl_scene() -> Scene {
        let mut mesh = modelview_core::Mesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            indices: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        };
        mesh.indices.pop();
        Scene {
            name: "broken".into(),
            source_path: None,
            format: Format::Stl,
            unit: LengthUnit::Millimeter,
            assumed_mm: true,
            objects: vec![SceneObject {
                name: "Mesh".into(),
                mesh,
                plate: None,
                source_id: None,
                color: None,
                filament: None,
            }],
            plates: vec![],
            project: None,
            warnings: vec![],
            byte_len: 0,
        }
    }

    #[test]
    fn stl_preview_forces_welding_even_when_disabled() {
        let scene = broken_stl_scene();
        let options = RepairOptions {
            weld: false,
            ..RepairOptions::default()
        };
        let preview = build_preview(&scene, options);
        assert_eq!(preview.summary.open_edges, 0);
        assert_eq!(preview.summary.triangles, 6);
        assert_eq!(preview.scene.objects[0].mesh.triangle_count(), 6);
    }

    #[test]
    fn preview_repairs_a_broken_scene_and_keeps_the_original() {
        let scene = broken_stl_scene();
        let before = modelview_core::analyze(&scene.objects[0].mesh, true);
        assert!(before.open_edges > 0);
        let preview = build_preview(&scene, RepairOptions::default());
        assert_eq!(preview.summary.open_edges, 0);
        assert_eq!(preview.summary.inconsistent_edges, 0);
        let after_original = modelview_core::analyze(&scene.objects[0].mesh, true);
        assert_eq!(after_original.open_edges, before.open_edges);
    }
}
