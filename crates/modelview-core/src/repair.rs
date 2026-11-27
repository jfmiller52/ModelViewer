use crate::analysis::{MeshReport, analyze};
use crate::mesh::{Mesh, weld};
use crate::scene::Scene;
use glam::Vec3;
use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Clone, Copy, Debug)]
pub struct RepairOptions {
    pub weld: bool,
    pub remove_degenerate: bool,
    pub remove_duplicate_faces: bool,
    pub heal_non_manifold: bool,
    pub fix_winding: bool,
    pub fill_holes: bool,
}

impl Default for RepairOptions {
    fn default() -> Self {
        Self {
            weld: true,
            remove_degenerate: true,
            remove_duplicate_faces: true,
            heal_non_manifold: true,
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
    pub non_manifold_edges_healed: usize,
    pub non_manifold_vertices_healed: usize,
    pub unresolved_non_manifold: usize,
    pub passes_rolled_back: usize,
    pub deep_remeshed: bool,
    pub voxel_size_mm: f32,
}

pub fn repair_mesh(mesh: &Mesh, options: RepairOptions) -> (Mesh, RepairStats) {
    let mut stats = RepairStats::default();
    let mut out = mesh.clone();
    let mut report = analyze(&out, false);

    let mut cleanup = out.clone();
    let mut cleanup_stats = RepairStats::default();
    let mut cleanup_passes = 0usize;
    if options.weld {
        let welded = weld(&cleanup);
        let removed = cleanup
            .positions
            .len()
            .saturating_sub(welded.positions.len());
        if removed > 0 {
            cleanup = welded;
            cleanup_stats.welded_vertices = removed;
            cleanup_passes += 1;
        }
    }
    if options.remove_degenerate {
        let mut delta = RepairStats::default();
        remove_invalid_and_degenerate(&mut cleanup, &mut delta);
        if delta.degenerate_removed > 0 {
            cleanup_stats.merge(&delta);
            cleanup_passes += 1;
        }
    }
    if options.remove_duplicate_faces {
        let mut delta = RepairStats::default();
        remove_duplicate_faces(&mut cleanup, &mut delta);
        if delta.duplicates_removed > 0 {
            cleanup_stats.merge(&delta);
            cleanup_passes += 1;
        }
    }
    if cleanup_passes > 0 {
        commit_group(
            &mut out,
            &mut report,
            &mut stats,
            cleanup,
            PassKind::Cleanup,
            cleanup_stats,
            cleanup_passes,
        );
    }

    if options.heal_non_manifold && report.non_manifold_edges + report.non_manifold_vertices > 0 {
        let before_edges = report.non_manifold_edges;
        let mut candidate = out.clone();
        let non_manifold_vertices_healed = split_non_manifold_vertices(&mut candidate);
        let non_manifold_edges_healed = before_edges.saturating_sub(count_non_manifold(&candidate));
        let delta = RepairStats {
            non_manifold_edges_healed,
            non_manifold_vertices_healed,
            ..Default::default()
        };
        if delta.non_manifold_vertices_healed > 0 || delta.non_manifold_edges_healed > 0 {
            commit_if_improved(
                &mut out,
                &mut report,
                &mut stats,
                candidate,
                PassKind::Heal,
                delta,
            );
        }
    }

    if options.fix_winding {
        let mut candidate = out.clone();
        let faces_flipped = fix_winding(&mut candidate);
        let delta = RepairStats {
            faces_flipped,
            ..Default::default()
        };
        if delta.faces_flipped > 0 {
            commit_if_improved(
                &mut out,
                &mut report,
                &mut stats,
                candidate,
                PassKind::Winding,
                delta,
            );
        }
    }

    if options.fill_holes {
        let mut candidate = out.clone();
        let mut delta = RepairStats::default();
        let filled = fill_holes(&mut candidate);
        delta.holes_filled = filled.holes;
        delta.patch_faces = filled.faces;
        delta.unfilled_loops = filled.unfilled;
        if delta.holes_filled > 0 {
            commit_if_improved(
                &mut out,
                &mut report,
                &mut stats,
                candidate,
                PassKind::Fill,
                delta,
            );
        }
    }

    remove_unreferenced(&mut out);
    stats.unresolved_non_manifold = report.non_manifold_edges + report.non_manifold_vertices;
    (out, stats)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PassKind {
    Cleanup,
    Heal,
    Winding,
    Fill,
}

fn commit_if_improved(
    out: &mut Mesh,
    report: &mut MeshReport,
    stats: &mut RepairStats,
    candidate: Mesh,
    kind: PassKind,
    delta: RepairStats,
) {
    commit_group(out, report, stats, candidate, kind, delta, 1);
}

fn commit_group(
    out: &mut Mesh,
    report: &mut MeshReport,
    stats: &mut RepairStats,
    candidate: Mesh,
    kind: PassKind,
    delta: RepairStats,
    changed_passes: usize,
) {
    let after = analyze(&candidate, false);
    if pass_improves(report, &after, kind) {
        *out = candidate;
        *report = after;
        stats.merge(&delta);
    } else {
        stats.passes_rolled_back += changed_passes;
    }
}

fn defect_score(report: &MeshReport) -> u64 {
    report.open_edges as u64
        + report.non_manifold_edges as u64 * 2
        + report.non_manifold_vertices as u64 * 2
        + report.inconsistent_edges as u64 * 2
        + report.degenerate_triangles as u64
        + report.invalid_triangles as u64 * 2
}

fn closed(report: &MeshReport) -> bool {
    report.open_edges == 0
        && report.non_manifold_edges == 0
        && report.non_manifold_vertices == 0
        && report.inconsistent_edges == 0
        && report.invalid_triangles == 0
}

fn volume_survives(before: &MeshReport, after: &MeshReport) -> bool {
    if !closed(before) || !closed(after) {
        return true;
    }
    let before_volume = before.volume_mm3;
    after.volume_mm3 >= before_volume * 0.5 || before_volume <= 1e-9
}

fn pass_improves(before: &MeshReport, after: &MeshReport, kind: PassKind) -> bool {
    if after.triangles == 0 && before.triangles > 0 {
        return false;
    }
    if !volume_survives(before, after) {
        return false;
    }
    let before_score = defect_score(before);
    let after_score = defect_score(after);
    match kind {
        PassKind::Cleanup => {
            after_score < before_score
                || (after_score == before_score
                    && (after.triangles < before.triangles || after.vertices < before.vertices))
        }
        PassKind::Heal => {
            after.open_edges <= before.open_edges
                && after_score < before_score
                && (after.non_manifold_edges + after.non_manifold_vertices)
                    < (before.non_manifold_edges + before.non_manifold_vertices)
        }
        PassKind::Winding => {
            after.open_edges <= before.open_edges
                && after.non_manifold_edges <= before.non_manifold_edges
                && after.non_manifold_vertices <= before.non_manifold_vertices
                && (after.inconsistent_edges < before.inconsistent_edges
                    || (after.inconsistent_edges == before.inconsistent_edges
                        && after.signed_volume_mm3 > before.signed_volume_mm3 + 1e-9))
        }
        PassKind::Fill => {
            after.open_edges < before.open_edges
                && after.non_manifold_edges <= before.non_manifold_edges
                && after.non_manifold_vertices <= before.non_manifold_vertices
                && after.inconsistent_edges <= before.inconsistent_edges
                && after.degenerate_triangles <= before.degenerate_triangles
                && after.invalid_triangles <= before.invalid_triangles
        }
    }
}

impl RepairStats {
    pub(crate) fn merge(&mut self, other: &Self) {
        self.welded_vertices += other.welded_vertices;
        self.degenerate_removed += other.degenerate_removed;
        self.duplicates_removed += other.duplicates_removed;
        self.faces_flipped += other.faces_flipped;
        self.holes_filled += other.holes_filled;
        self.patch_faces += other.patch_faces;
        self.unfilled_loops += other.unfilled_loops;
        self.non_manifold_edges_healed += other.non_manifold_edges_healed;
        self.non_manifold_vertices_healed += other.non_manifold_vertices_healed;
        self.unresolved_non_manifold += other.unresolved_non_manifold;
        self.passes_rolled_back += other.passes_rolled_back;
        self.deep_remeshed |= other.deep_remeshed;
        if other.voxel_size_mm > 0.0 {
            self.voxel_size_mm = other.voxel_size_mm;
        }
    }
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

#[derive(Clone, Copy, Default)]
struct RepairEdge {
    count: u32,
    tri_a: u32,
    tri_b: u32,
}

struct SplitVertex {
    corners: Vec<u32>,
    labels: Vec<u32>,
    ids: Vec<u32>,
}

fn split_non_manifold_vertices(mesh: &mut Mesh) -> usize {
    let vertex_count = mesh.positions.len();
    let triangle_count = mesh.indices.len();
    if vertex_count == 0 || triangle_count == 0 {
        return 0;
    }

    let edges = repair_edges(mesh);
    let (offsets, incident) = incident_corners(mesh, vertex_count);
    let mut visited = vec![0u32; triangle_count];
    let mut labels = vec![0u32; triangle_count];
    let mut stamp = 0u32;
    let mut splits = Vec::new();
    let mut stack = Vec::new();

    for vertex in 0..vertex_count {
        let start = offsets[vertex];
        let end = offsets[vertex + 1];
        if end.saturating_sub(start) < 2 {
            continue;
        }
        stamp = stamp.wrapping_add(1);
        if stamp == 0 {
            visited.fill(0);
            stamp = 1;
        }
        let mut components = 0u32;
        for &corner in &incident[start..end] {
            let triangle = corner / 3;
            if visited[triangle as usize] == stamp {
                continue;
            }
            components += 1;
            visited[triangle as usize] = stamp;
            labels[triangle as usize] = components;
            stack.push(triangle);
            while let Some(current) = stack.pop() {
                let tri = mesh.indices[current as usize];
                let Some(corner) = tri.iter().position(|index| *index as usize == vertex) else {
                    continue;
                };
                let next = tri[(corner + 1) % 3];
                let previous = tri[(corner + 2) % 3];
                for other in [next, previous] {
                    let key = edge_key(vertex as u32, other);
                    let Some(edge) = edges.get(&key) else {
                        continue;
                    };
                    if edge.count != 2 {
                        continue;
                    }
                    let neighbor = if edge.tri_a == current {
                        edge.tri_b
                    } else if edge.tri_b == current {
                        edge.tri_a
                    } else {
                        continue;
                    };
                    if neighbor != current && visited[neighbor as usize] != stamp {
                        visited[neighbor as usize] = stamp;
                        labels[neighbor as usize] = components;
                        stack.push(neighbor);
                    }
                }
            }
        }
        if components < 2 {
            continue;
        }
        let mut ids = Vec::with_capacity(components as usize);
        ids.push(vertex as u32);
        for _ in 1..components {
            let id = mesh.positions.len() as u32;
            mesh.positions.push(mesh.positions[vertex]);
            ids.push(id);
        }
        let mut corners = Vec::with_capacity(end - start);
        let mut split_labels = Vec::with_capacity(end - start);
        for &corner in &incident[start..end] {
            corners.push(corner);
            split_labels.push(labels[(corner / 3) as usize]);
        }
        splits.push(SplitVertex {
            corners,
            labels: split_labels,
            ids,
        });
    }

    let count = splits.len();
    for split in splits {
        for (corner, label) in split.corners.into_iter().zip(split.labels) {
            let triangle = (corner / 3) as usize;
            let corner = (corner % 3) as usize;
            mesh.indices[triangle][corner] = split.ids[(label - 1) as usize];
        }
    }
    count
}

fn repair_edges(mesh: &Mesh) -> FxHashMap<u64, RepairEdge> {
    let mut edges: FxHashMap<u64, RepairEdge> = FxHashMap::default();
    for (triangle, tri) in mesh.indices.iter().enumerate() {
        if tri
            .iter()
            .any(|index| (*index as usize) >= mesh.positions.len())
        {
            continue;
        }
        for (a, b) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
            let edge = edges.entry(edge_key(a, b)).or_default();
            edge.count += 1;
            if edge.count == 1 {
                edge.tri_a = triangle as u32;
            } else if edge.count == 2 {
                edge.tri_b = triangle as u32;
            }
        }
    }
    edges
}

fn incident_corners(mesh: &Mesh, vertex_count: usize) -> (Vec<usize>, Vec<u32>) {
    let mut counts = vec![0usize; vertex_count];
    for tri in &mesh.indices {
        if tri.iter().any(|index| (*index as usize) >= vertex_count) {
            continue;
        }
        for index in tri {
            counts[*index as usize] += 1;
        }
    }
    let mut offsets = vec![0usize; vertex_count + 1];
    for vertex in 0..vertex_count {
        offsets[vertex + 1] = offsets[vertex] + counts[vertex];
    }
    let mut cursors = offsets[..vertex_count].to_vec();
    let mut corners = vec![0u32; offsets[vertex_count]];
    for (triangle, tri) in mesh.indices.iter().enumerate() {
        if tri.iter().any(|index| (*index as usize) >= vertex_count) {
            continue;
        }
        for (corner, vertex) in tri.iter().enumerate() {
            let vertex = *vertex as usize;
            corners[cursors[vertex]] = (triangle * 3 + corner) as u32;
            cursors[vertex] += 1;
        }
    }
    (offsets, corners)
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
            heal_non_manifold: true,
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

    fn two_tetrahedra_sharing_edge() -> Mesh {
        let mut mesh = tetra();
        mesh.positions.extend([[0.0, -1.0, 0.0], [0.0, 0.0, -1.0]]);
        mesh.indices
            .extend([[0, 4, 1], [0, 1, 5], [0, 5, 4], [1, 4, 5]]);
        mesh
    }

    fn expanded(mesh: &Mesh) -> Mesh {
        let mut expanded = Mesh::default();
        for tri in &mesh.indices {
            let base = expanded.positions.len() as u32;
            for corner in tri {
                expanded.positions.push(mesh.positions[*corner as usize]);
            }
            expanded.indices.push([base, base + 1, base + 2]);
        }
        expanded
    }

    fn tetra_with_fin() -> Mesh {
        let mut mesh = tetra();
        mesh.positions.push([2.0, 2.0, 2.0]);
        mesh.indices.push([0, 1, 4]);
        mesh
    }

    fn two_tetrahedra_sharing_vertex() -> Mesh {
        let mut mesh = tetra();
        mesh.positions
            .extend([[-1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, -1.0]]);
        mesh.indices
            .extend([[0, 5, 4], [0, 4, 6], [0, 6, 5], [4, 5, 6]]);
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
                heal_non_manifold: false,
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
    fn unwelded_stl_is_welded_before_other_passes() {
        let mut mesh = tetra();
        mesh.indices[0].swap(1, 2);
        let expanded = expanded(&mesh);

        let (out, stats) = repair_mesh(&expanded, repair_all());
        let report = analyze(&out, false);

        assert_eq!(report.open_edges, 0);
        assert_eq!(report.inconsistent_edges, 0);
        assert_eq!(report.shells, 1);
        assert_eq!(report.triangles, 4);
        assert!(stats.welded_vertices > 0);
        assert_eq!(stats.holes_filled, 0);
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
    fn shared_edge_between_shells_is_split() {
        let mesh = two_tetrahedra_sharing_edge();
        let before = analyze(&mesh, false);
        assert_eq!(before.open_edges, 0);
        assert_eq!(before.non_manifold_edges, 1);

        let (out, stats) = repair_mesh(&mesh, repair_all());
        let report = analyze(&out, false);

        assert_eq!(report.open_edges, 0);
        assert_eq!(report.non_manifold_edges, 0);
        assert_eq!(report.non_manifold_vertices, 0);
        assert_eq!(report.shells, 2);
        assert_eq!(stats.unresolved_non_manifold, 0);
    }

    #[test]
    fn healing_does_not_turn_a_fin_into_a_closed_shell() {
        let mesh = tetra_with_fin();
        let before = analyze(&mesh, false);
        assert_eq!(before.non_manifold_edges, 1);

        let (out, stats) = repair_mesh(&mesh, repair_all());
        let report = analyze(&out, false);

        assert!(report.non_manifold_edges > 0);
        assert!(!report.is_watertight());
        assert_eq!(stats.non_manifold_edges_healed, 0);
        assert!(stats.passes_rolled_back >= 1);
    }

    #[test]
    fn shared_vertex_between_shells_is_split() {
        let mesh = two_tetrahedra_sharing_vertex();
        let before = analyze(&mesh, false);
        assert_eq!(before.open_edges, 0);
        assert_eq!(before.non_manifold_vertices, 1);

        let (out, stats) = repair_mesh(&mesh, repair_all());
        let report = analyze(&out, false);

        assert_eq!(report.open_edges, 0);
        assert_eq!(report.non_manifold_edges, 0);
        assert_eq!(report.non_manifold_vertices, 0);
        assert_eq!(report.shells, 2);
        assert_eq!(stats.unresolved_non_manifold, 0);
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
