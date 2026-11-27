use crate::analysis::analyze;
use crate::mesh::{Mesh, weld};
use crate::repair::{RepairOptions, RepairStats, repair_mesh};
use crate::scene::Scene;
use anyhow::{Context, Result, anyhow, bail};
use baby_shark::exports::nalgebra::Vector3;
use baby_shark::mesh::polygon_soup::data_structure::PolygonSoup;
use baby_shark::remeshing::voxel::{MeshingMethod, VoxelRemesher};

const VOXELS_PER_DIAGONAL: f32 = 160.0;
const MIN_VOXEL_MM: f32 = 0.02;
const MAX_VOXEL_MM: f32 = 2.0;

pub fn deep_repair_mesh(mesh: &Mesh) -> Result<(Mesh, RepairStats)> {
    if mesh.indices.is_empty() || mesh.positions.is_empty() {
        bail!("Deep repair needs triangles to rebuild.");
    }
    let prepared = prepare_input(mesh);
    let (prepared, mut stats) = prepared;
    if prepared.indices.is_empty() {
        bail!("Deep repair removed every triangle.");
    }

    let bounds = prepared.bounds();
    let diagonal = bounds.diagonal();
    if !diagonal.is_finite() || diagonal <= 1e-6 {
        bail!("Deep repair could not measure the model.");
    }
    let voxel_size = (diagonal / VOXELS_PER_DIAGONAL).clamp(MIN_VOXEL_MM, MAX_VOXEL_MM);

    let soup = to_soup(&prepared);
    let mut remesher = VoxelRemesher::default()
        .with_voxel_size(voxel_size)
        .with_meshing_method(MeshingMethod::Manifold);
    let remeshed = remesher
        .remesh(&soup)
        .ok_or_else(|| anyhow!("Deep repair could not build a voxel field."))?;
    let out = from_soup(&remeshed);
    if out.indices.is_empty() || out.positions.is_empty() {
        bail!("Deep repair produced no geometry.");
    }
    let out = weld(&out);
    let (out, clean_stats) = repair_mesh(&out, RepairOptions::default());
    stats.merge(&clean_stats);
    let report = analyze(&out, false);
    if !report.is_watertight() {
        bail!(
            "Deep repair still has {} open edge(s) and {} non-manifold join(s).",
            report.open_edges,
            report.non_manifold_edges + report.non_manifold_vertices
        );
    }
    if report.volume_mm3 <= 1e-9 {
        bail!("Deep repair collapsed the model's volume.");
    }

    let before = analyze(&prepared, false);
    if before.volume_mm3 > 1e-9 {
        let ratio = report.volume_mm3 / before.volume_mm3;
        if !(0.25..=4.0).contains(&ratio) {
            bail!(
                "Deep repair changed the volume too much ({:.0}%).",
                (ratio - 1.0) * 100.0
            );
        }
    }

    stats.deep_remeshed = true;
    stats.voxel_size_mm = voxel_size;
    stats.unresolved_non_manifold = 0;
    Ok((out, stats))
}

fn prepare_input(mesh: &Mesh) -> (Mesh, RepairStats) {
    repair_mesh(
        mesh,
        RepairOptions {
            weld: true,
            remove_degenerate: true,
            remove_duplicate_faces: true,
            heal_non_manifold: true,
            fix_winding: true,
            fill_holes: false,
        },
    )
}

fn to_soup(mesh: &Mesh) -> PolygonSoup<f32> {
    let vertices: Vec<Vector3<f32>> = mesh
        .positions
        .iter()
        .map(|p| Vector3::new(p[0], p[1], p[2]))
        .collect();
    let faces: Vec<usize> = mesh
        .indices
        .iter()
        .flat_map(|t| [t[0] as usize, t[1] as usize, t[2] as usize])
        .collect();
    PolygonSoup::from_vertex_and_face_slices(&vertices, &faces)
}

fn from_soup(soup: &PolygonSoup<f32>) -> Mesh {
    let positions: Vec<[f32; 3]> = soup.vertices().map(|v| [v.x, v.y, v.z]).collect();
    let indices = (0..positions.len() / 3)
        .map(|i| {
            let base = (i * 3) as u32;
            [base, base + 1, base + 2]
        })
        .collect();
    Mesh { positions, indices }
}

pub fn deep_repair_scene(scene: &Scene) -> Result<(Scene, Vec<RepairStats>)> {
    let mut out = scene.clone();
    let mut stats = Vec::with_capacity(out.objects.len());
    for object in &mut out.objects {
        let (mesh, object_stats) = deep_repair_mesh(&object.mesh)
            .with_context(|| format!("Deep repair failed for {}.", object.name))?;
        object.mesh = mesh;
        stats.push(object_stats);
    }
    Ok((out, stats))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::analyze;
    use crate::mesh::Mesh;

    fn cube(size: f32) -> Mesh {
        let positions = vec![
            [0.0, 0.0, 0.0],
            [size, 0.0, 0.0],
            [size, size, 0.0],
            [0.0, size, 0.0],
            [0.0, 0.0, size],
            [size, 0.0, size],
            [size, size, size],
            [0.0, size, size],
        ];
        let indices = vec![
            [0, 3, 2],
            [0, 2, 1],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [1, 2, 6],
            [1, 6, 5],
            [2, 3, 7],
            [2, 7, 6],
            [3, 0, 4],
            [3, 4, 7],
        ];
        Mesh { positions, indices }
    }

    fn open_cube(size: f32) -> Mesh {
        let mut mesh = cube(size);
        mesh.indices.pop();
        mesh
    }

    fn scene(mesh: Mesh) -> Scene {
        use crate::scene::{Format, LengthUnit, SceneObject};
        Scene {
            name: "deep".into(),
            source_path: None,
            format: Format::ThreeMf,
            unit: LengthUnit::Millimeter,
            assumed_mm: false,
            objects: vec![SceneObject {
                name: "shell".into(),
                mesh,
                plate: Some(3),
                source_id: Some(7),
                color: Some([10, 20, 30]),
                filament: Some(2),
            }],
            plates: vec![],
            project: None,
            warnings: vec![],
            byte_len: 0,
        }
    }

    #[test]
    fn deep_repair_returns_a_watertight_mesh() {
        let (out, stats) = deep_repair_mesh(&cube(10.0)).unwrap();
        let report = analyze(&out, false);
        assert!(report.is_watertight());
        assert!(report.volume_mm3 > 0.0);
        assert!(stats.deep_remeshed);
        assert!(stats.voxel_size_mm > 0.0);
    }

    #[test]
    fn deep_repair_closes_an_open_cube() {
        let (out, _) = deep_repair_mesh(&open_cube(10.0)).unwrap();
        let report = analyze(&out, false);
        assert!(report.is_watertight());
    }

    #[test]
    fn deep_repair_rejects_an_empty_mesh() {
        assert!(deep_repair_mesh(&Mesh::default()).is_err());
    }

    #[test]
    fn deep_repair_scene_keeps_metadata() {
        let (out, stats) = deep_repair_scene(&scene(cube(10.0))).unwrap();
        assert_eq!(out.objects[0].name, "shell");
        assert_eq!(out.objects[0].plate, Some(3));
        assert_eq!(out.objects[0].source_id, Some(7));
        assert_eq!(out.objects[0].color, Some([10, 20, 30]));
        assert_eq!(out.objects[0].filament, Some(2));
        assert_eq!(stats.len(), 1);
        assert!(stats[0].deep_remeshed);
    }
}
