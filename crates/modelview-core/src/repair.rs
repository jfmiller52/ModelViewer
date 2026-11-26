use crate::mesh::{Mesh, weld};
use crate::scene::Scene;
use glam::Vec3;
use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Clone, Copy, Debug)]
pub struct RepairOptions {
    pub weld: bool,
    pub remove_degenerate: bool,
    pub remove_duplicate_faces: bool,
    pub fix_winding: bool,
    pub fill_holes: bool,
}

impl Default for RepairOptions {
    fn default() -> Self {
        Self {
            weld: true,
            remove_degenerate: true,
            remove_duplicate_faces: true,
            fix_winding: true,
            fill_holes: true,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct RepairStats {
    pub welded_vertices: usize,
    pub degenerate_removed: usize,
    pub duplicates_removed: usize,
    pub faces_flipped: usize,
    pub holes_filled: usize,
    pub patch_faces: usize,
    pub unfilled_loops: usize,
    pub unresolved_non_manifold: usize,
}

pub fn repair_mesh(mesh: &Mesh, options: RepairOptions) -> (Mesh, RepairStats) {
    let mut stats = RepairStats::default();
    let mut out = if options.weld {
        let welded = weld(mesh);
        stats.welded_vertices = mesh.positions.len().saturating_sub(welded.positions.len());
        welded
    } else {
        mesh.clone()
    };

    if options.remove_degenerate {
        remove_invalid_and_degenerate(&mut out, &mut stats);
    }
    if options.remove_duplicate_faces {
        remove_duplicate_faces(&mut out, &mut stats);
    }
    if options.fix_winding {
        stats.faces_flipped = fix_winding(&mut out);
    }
    if options.fill_holes {
        let filled = fill_holes(&mut out);
        stats.holes_filled = filled.holes;
        stats.patch_faces = filled.faces;
        stats.unfilled_loops = filled.unfilled;
    }
    remove_unreferenced(&mut out);
    stats.unresolved_non_manifold = count_non_manifold(&out);
    (out, stats)
}

fn triangle_area2(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> f64 {
    let ux = (b[0] - a[0]) as f64;
    let uy = (b[1] - a[1]) as f64;
    let uz = (b[2] - a[2]) as f64;
    let vx = (c[0] - a[0]) as f64;
    let vy = (c[1] - a[1]) as f64;
    let vz = (c[2] - a[2]) as f64;
    let cx = uy * vz - uz * vy;
    let cy = uz * vx - ux * vz;
    let cz = ux * vy - uy * vx;
    (cx * cx + cy * cy + cz * cz).sqrt()
}

fn remove_invalid_and_degenerate(mesh: &mut Mesh, stats: &mut RepairStats) {
    let count = mesh.positions.len();
    let positions = &mesh.positions;
    let mut removed = 0;
    mesh.indices.retain(|t| {
        let valid = (t[0] as usize) < count && (t[1] as usize) < count && (t[2] as usize) < count;
        if !valid {
            removed += 1;
            return false;
        }
        let a = positions[t[0] as usize];
        let b = positions[t[1] as usize];
        let c = positions[t[2] as usize];
        if triangle_area2(a, b, c) <= 1e-9 {
            removed += 1;
            false
        } else {
            true
        }
    });
    stats.degenerate_removed += removed;
}

fn remove_duplicate_faces(mesh: &mut Mesh, stats: &mut RepairStats) {
    let mut seen: FxHashSet<[u32; 3]> = FxHashSet::default();
    mesh.indices.retain(|t| {
        let mut key = *t;
        key.sort_unstable();
        if seen.insert(key) {
            true
        } else {
            stats.duplicates_removed += 1;
            false
        }
    });
}

fn remove_unreferenced(mesh: &mut Mesh) {
    if mesh.positions.is_empty() {
        return;
    }
    let mut used = vec![false; mesh.positions.len()];
    for t in &mesh.indices {
        used[t[0] as usize] = true;
        used[t[1] as usize] = true;
        used[t[2] as usize] = true;
    }
    if used.iter().all(|u| *u) {
        return;
    }
    let mut map = vec![0u32; mesh.positions.len()];
    let mut positions = Vec::with_capacity(mesh.positions.len());
    for (index, keep) in used.into_iter().enumerate() {
        if keep {
            map[index] = positions.len() as u32;
            positions.push(mesh.positions[index]);
        }
    }
    for t in &mut mesh.indices {
        t[0] = map[t[0] as usize];
        t[1] = map[t[1] as usize];
        t[2] = map[t[2] as usize];
    }
    mesh.positions = positions;
}

fn edge_key(a: u32, b: u32) -> u64 {
    if a < b {
        ((a as u64) << 32) | b as u64
    } else {
        ((b as u64) << 32) | a as u64
    }
}

fn count_non_manifold(mesh: &Mesh) -> usize {
    let mut counts: FxHashMap<u64, u8> = FxHashMap::default();
    for t in &mesh.indices {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            *counts.entry(edge_key(a, b)).or_default() += 1;
        }
    }
    counts.values().filter(|c| **c > 2).count()
}

struct HoleFill {
    holes: usize,
    faces: usize,
    unfilled: usize,
}

pub fn repair_scene(scene: &Scene, options: RepairOptions) -> (Scene, Vec<RepairStats>) {
    let mut out = scene.clone();
    let mut stats = Vec::with_capacity(out.objects.len());
    for object in &mut out.objects {
        let (mesh, object_stats) = repair_mesh(&object.mesh, options);
        object.mesh = mesh;
        stats.push(object_stats);
    }
    (out, stats)
}

fn fix_winding(mesh: &mut Mesh) -> usize {
    let n = mesh.indices.len();
    if n == 0 {
        return 0;
    }
    let mut edges: FxHashMap<u64, Vec<(u32, bool)>> = FxHashMap::default();
    for (i, t) in mesh.indices.iter().enumerate() {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            edges
                .entry(edge_key(a, b))
                .or_default()
                .push((i as u32, a < b));
        }
    }
    let mut flip = vec![false; n];
    let mut visited = vec![false; n];
    let mut components: Vec<Vec<u32>> = Vec::new();
    for start in 0..n {
        if visited[start] {
            continue;
        }
        visited[start] = true;
        let mut component = Vec::new();
        let mut queue = vec![start as u32];
        while let Some(face) = queue.pop() {
            component.push(face);
            let t = mesh.indices[face as usize];
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                let pair = match edges.get(&edge_key(a, b)) {
                    Some(pair) if pair.len() == 2 => pair,
                    _ => continue,
                };
                let current_dir = pair
                    .iter()
                    .find(|(other, _)| *other == face)
                    .map(|(_, dir)| *dir);
                let other = pair.iter().find(|(other, _)| *other != face);
                if let (Some(current_dir), Some((other, other_dir))) = (current_dir, other) {
                    let expected = if current_dir == *other_dir {
                        !flip[face as usize]
                    } else {
                        flip[face as usize]
                    };
                    if !visited[*other as usize] {
                        visited[*other as usize] = true;
                        flip[*other as usize] = expected;
                        queue.push(*other);
                    }
                }
            }
        }
        components.push(component);
    }

    let mut flipped = 0usize;
    for face in 0..n {
        if flip[face] {
            mesh.indices[face].swap(1, 2);
            flipped += 1;
        }
    }

    let volumes: Vec<f64> = components
        .iter()
        .map(|component| component_volume(mesh, component))
        .collect();
    let closed: Vec<bool> = components
        .iter()
        .map(|component| {
            component.iter().all(|face| {
                let t = mesh.indices[*face as usize];
                [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])]
                    .iter()
                    .all(|(a, b)| {
                        edges
                            .get(&edge_key(*a, *b))
                            .is_some_and(|pair| pair.len() == 2)
                    })
            })
        })
        .collect();
    let depths: Vec<usize> = components
        .iter()
        .enumerate()
        .map(|(index, component)| {
            if !closed[index] || volumes[index].abs() <= 1e-9 {
                return 0;
            }
            let first_face = component[0] as usize;
            let point = Vec3::from_array(mesh.positions[mesh.indices[first_face][0] as usize]);
            components
                .iter()
                .enumerate()
                .filter(|(other_index, other)| {
                    *other_index != index
                        && closed[*other_index]
                        && volumes[*other_index].abs() > 1e-9
                        && point_in_component(point, mesh, other)
                })
                .count()
        })
        .collect();
    for (index, component) in components.iter().enumerate() {
        if closed[index] && volumes[index].abs() > 1e-9 {
            let want_inward = depths[index] % 2 == 1;
            if want_inward != (volumes[index] < 0.0) {
                for face in component {
                    mesh.indices[*face as usize].swap(1, 2);
                }
                flipped += component.len();
            }
            continue;
        }
        if volumes[index] >= -1e-9 {
            continue;
        }
        let first_face = component[0] as usize;
        let point = Vec3::from_array(mesh.positions[mesh.indices[first_face][0] as usize]);
        let nested = components.iter().enumerate().any(|(other_index, other)| {
            volumes[other_index] > 1e-9 && point_in_component(point, mesh, other)
        });
        if !nested {
            for face in component {
                mesh.indices[*face as usize].swap(1, 2);
            }
            flipped += component.len();
        }
    }
    flipped
}

fn component_volume(mesh: &Mesh, component: &[u32]) -> f64 {
    component
        .iter()
        .map(|face| {
            let t = mesh.indices[*face as usize];
            let a = Vec3::from_array(mesh.positions[t[0] as usize]);
            let b = Vec3::from_array(mesh.positions[t[1] as usize]);
            let c = Vec3::from_array(mesh.positions[t[2] as usize]);
            a.dot(b.cross(c)) as f64 / 6.0
        })
        .sum()
}

fn point_in_component(point: Vec3, mesh: &Mesh, component: &[u32]) -> bool {
    let direction = Vec3::new(0.577_350_26, 0.211_324_86, 0.788_675_1).normalize();
    let mut crossings = 0usize;
    for face in component {
        let t = mesh.indices[*face as usize];
        let a = Vec3::from_array(mesh.positions[t[0] as usize]);
        let b = Vec3::from_array(mesh.positions[t[1] as usize]);
        let c = Vec3::from_array(mesh.positions[t[2] as usize]);
        if ray_triangle(point, direction, a, b, c) {
            crossings += 1;
        }
    }
    crossings % 2 == 1
}

fn ray_triangle(origin: Vec3, direction: Vec3, a: Vec3, b: Vec3, c: Vec3) -> bool {
    let e1 = b - a;
    let e2 = c - a;
    let h = direction.cross(e2);
    let det = e1.dot(h);
    if det.abs() < 1e-8 {
        return false;
    }
    let inv = 1.0 / det;
    let s = origin - a;
    let u = inv * s.dot(h);
    if !(0.0..=1.0).contains(&u) {
        return false;
    }
    let q = s.cross(e1);
    let v = inv * direction.dot(q);
    if v < 0.0 || u + v > 1.0 {
        return false;
    }
    let t = inv * e2.dot(q);
    t > 1e-5
}

fn dir_key(a: u32, b: u32) -> u64 {
    ((a as u64) << 32) | b as u64
}

fn fill_holes(mesh: &mut Mesh) -> HoleFill {
    let mut edges: FxHashMap<u64, (u8, u32, u32)> = FxHashMap::default();
    for t in &mesh.indices {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let entry = edges.entry(edge_key(a, b)).or_insert((0, a, b));
            entry.0 = entry.0.saturating_add(1);
        }
    }
    let boundary: Vec<(u32, u32)> = edges
        .values()
        .filter(|(count, _, _)| *count == 1)
        .map(|(_, a, b)| (*a, *b))
        .collect();
    if boundary.is_empty() {
        return HoleFill {
            holes: 0,
            faces: 0,
            unfilled: 0,
        };
    }

    let mut outgoing: FxHashMap<u32, Vec<u32>> = FxHashMap::default();
    for (a, b) in &boundary {
        outgoing.entry(*a).or_default().push(*b);
    }
    let mut used: FxHashSet<u64> = FxHashSet::default();
    let mut holes = 0usize;
    let mut faces = 0usize;
    let mut unfilled = 0usize;

    for &(start_a, start_b) in &boundary {
        if used.contains(&dir_key(start_a, start_b)) {
            continue;
        }
        let mut loop_edges: Vec<(u32, u32)> = Vec::new();
        let mut a = start_a;
        let mut b = start_b;
        let mut closed = false;
        for _ in 0..=boundary.len() {
            if !used.insert(dir_key(a, b)) {
                break;
            }
            loop_edges.push((a, b));
            if b == start_a {
                closed = true;
                break;
            }
            let Some(nexts) = outgoing.get(&b) else {
                break;
            };
            let Some(&next) = nexts
                .iter()
                .find(|next| !used.contains(&dir_key(b, **next)))
            else {
                break;
            };
            a = b;
            b = next;
        }
        if !closed || loop_edges.len() < 3 {
            unfilled += 1;
            continue;
        }
        let mut vertices: Vec<u32> = loop_edges.iter().map(|(a, _)| *a).collect();
        vertices.sort_unstable();
        vertices.dedup();
        if vertices.len() < 3 {
            unfilled += 1;
            continue;
        }
        let mut sum = Vec3::ZERO;
        for vertex in &vertices {
            sum += Vec3::from_array(mesh.positions[*vertex as usize]);
        }
        let centroid = sum / vertices.len() as f32;
        let valid = loop_edges.iter().all(|(a, b)| {
            let pa = mesh.positions[*a as usize];
            let pb = mesh.positions[*b as usize];
            triangle_area2(pa, pb, centroid.to_array()) > 1e-9
        });
        if !valid {
            unfilled += 1;
            continue;
        }
        let centroid_index = mesh.positions.len() as u32;
        mesh.positions.push(centroid.to_array());
        for &(a, b) in &loop_edges {
            mesh.indices.push([b, a, centroid_index]);
            faces += 1;
        }
        holes += 1;
    }

    HoleFill {
        holes,
        faces,
        unfilled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weld_dedup_and_degenerate_mesh() -> Mesh {
        Mesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 0.0],
            ],
            indices: vec![
                [0, 2, 1],
                [0, 1, 3],
                [0, 3, 2],
                [1, 2, 3],
                [4, 2, 1],
                [0, 0, 1],
            ],
        }
    }

    use crate::analysis::analyze;
    use crate::scene::{Format, LengthUnit, Plate, SceneObject};

    fn tetra() -> Mesh {
        Mesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            indices: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        }
    }

    fn repair_all() -> RepairOptions {
        RepairOptions::default()
    }

    fn repair_all_but_holes() -> RepairOptions {
        RepairOptions {
            weld: true,
            remove_degenerate: true,
            remove_duplicate_faces: true,
            fix_winding: true,
            fill_holes: false,
        }
    }

    fn cube(size: f32, offset: [f32; 3], inverted: bool) -> Mesh {
        let positions = vec![
            [offset[0], offset[1], offset[2]],
            [offset[0] + size, offset[1], offset[2]],
            [offset[0] + size, offset[1] + size, offset[2]],
            [offset[0], offset[1] + size, offset[2]],
            [offset[0], offset[1], offset[2] + size],
            [offset[0] + size, offset[1], offset[2] + size],
            [offset[0] + size, offset[1] + size, offset[2] + size],
            [offset[0], offset[1] + size, offset[2] + size],
        ];
        let mut indices = vec![
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
        if inverted {
            for t in &mut indices {
                t.swap(1, 2);
            }
        }
        Mesh { positions, indices }
    }

    fn two_nested_cubes(outer: f32, inner: f32) -> Mesh {
        let mut mesh = cube(outer, [0.0, 0.0, 0.0], false);
        let offset = (outer - inner) * 0.5;
        mesh.append(
            &cube(inner, [offset, offset, offset], true),
            &glam::Mat4::IDENTITY,
        );
        mesh
    }

    fn warped_quad() -> Mesh {
        Mesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.5],
                [0.0, 1.0, 0.0],
            ],
            indices: vec![[0, 1, 2], [0, 2, 3]],
        }
    }

    #[test]
    fn weld_dedup_and_degenerate_are_counted() {
        let mesh = weld_dedup_and_degenerate_mesh();
        let (out, stats) = repair_mesh(
            &mesh,
            RepairOptions {
                weld: true,
                remove_degenerate: true,
                remove_duplicate_faces: true,
                fix_winding: false,
                fill_holes: false,
            },
        );
        assert_eq!(stats.welded_vertices, 1);
        assert_eq!(stats.degenerate_removed, 1);
        assert_eq!(stats.duplicates_removed, 1);
        assert_eq!(out.triangle_count(), 4);
    }

    #[test]
    fn flipped_tetra_face_is_repaired() {
        let mut mesh = tetra();
        mesh.indices[0] = [0, 1, 2];
        let (out, stats) = repair_mesh(&mesh, repair_all_but_holes());
        let r = analyze(&out, false);
        assert_eq!(r.open_edges, 0);
        assert_eq!(r.inconsistent_edges, 0);
        assert!(r.signed_volume_mm3 > 0.0);
        assert!(stats.faces_flipped >= 1);
    }

    #[test]
    fn nested_cavity_keeps_inward_orientation() {
        let mesh = two_nested_cubes(10.0, 4.0);
        let (out, _) = repair_mesh(&mesh, repair_all_but_holes());
        let r = analyze(&out, false);
        assert_eq!(r.open_edges, 0);
        assert_eq!(r.shells, 2);
        assert!((r.signed_volume_mm3 - (1000.0 - 64.0)).abs() < 1.0);
    }

    #[test]
    fn mixed_winding_cavity_stays_inward() {
        let mut mesh = two_nested_cubes(10.0, 4.0);
        mesh.indices[12].swap(1, 2);
        let (out, _) = repair_mesh(&mesh, repair_all_but_holes());
        let r = analyze(&out, false);
        assert_eq!(r.open_edges, 0);
        assert_eq!(r.shells, 2);
        assert!((r.signed_volume_mm3 - (1000.0 - 64.0)).abs() < 1.0);
    }

    #[test]
    fn missing_tetra_face_is_filled() {
        let mut mesh = tetra();
        mesh.indices.pop();
        let (out, stats) = repair_mesh(&mesh, repair_all());
        let r = analyze(&out, false);
        assert_eq!(r.open_edges, 0);
        assert_eq!(r.inconsistent_edges, 0);
        assert_eq!(stats.holes_filled, 1);
        assert!(stats.patch_faces >= 3);
        assert!((r.volume_mm3 - 1.0 / 6.0).abs() < 1e-6);
    }

    #[test]
    fn non_planar_loop_does_not_crash_or_claim_watertight() {
        let mesh = warped_quad();
        let (out, stats) = repair_mesh(&mesh, repair_all());
        let r = analyze(&out, false);
        if stats.unfilled_loops == 0 {
            assert_eq!(r.open_edges, 0);
        } else {
            assert!(r.open_edges > 0);
        }
    }

    #[test]
    fn non_manifold_edges_remain_reported_not_repaired() {
        let mut mesh = tetra();
        mesh.positions.push([2.0, 2.0, 2.0]);
        mesh.indices.push([0, 1, 4]);
        let (out, stats) = repair_mesh(&mesh, repair_all());
        let report = analyze(&out, false);
        assert!(report.non_manifold_edges > 0);
        assert!(stats.unresolved_non_manifold > 0);
        assert_ne!(report.verdict(), crate::analysis::Verdict::Watertight);
    }

    #[test]
    fn repairs_each_object_and_keeps_names_and_plates() {
        let mut broken = tetra();
        broken.indices.pop();
        let scene = Scene {
            name: "two".into(),
            source_path: None,
            format: Format::ThreeMf,
            unit: LengthUnit::Millimeter,
            assumed_mm: false,
            objects: vec![
                SceneObject {
                    name: "broken".into(),
                    mesh: broken,
                    plate: Some(1),
                    source_id: Some(7),
                    color: None,
                    filament: None,
                },
                SceneObject {
                    name: "clean".into(),
                    mesh: tetra(),
                    plate: Some(2),
                    source_id: Some(9),
                    color: None,
                    filament: None,
                },
            ],
            plates: vec![
                Plate {
                    id: 1,
                    name: "Plate 01".into(),
                    objects: vec![0],
                },
                Plate {
                    id: 2,
                    name: "Plate 02".into(),
                    objects: vec![1],
                },
            ],
            project: None,
            warnings: vec![],
            byte_len: 0,
        };
        let (out, stats) = repair_scene(&scene, repair_all());
        assert_eq!(out.objects[0].name, "broken");
        assert_eq!(out.objects[0].plate, Some(1));
        assert_eq!(out.objects[1].source_id, Some(9));
        assert_eq!(stats.len(), 2);
        assert_eq!(analyze(&out.objects[0].mesh, false).open_edges, 0);
        assert_eq!(analyze(&out.objects[1].mesh, false).open_edges, 0);
    }
}
