use crate::mesh::{Bounds, Mesh, weld};
use glam::Vec3;
use rayon::prelude::*;
use rustc_hash::FxHashMap;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefectKind {
    OpenEdge,
    NonManifold,
    InconsistentWinding,
    DegenerateFace,
}

impl DefectKind {
    pub fn label(self) -> &'static str {
        match self {
            DefectKind::OpenEdge => "Open edge",
            DefectKind::NonManifold => "Non-manifold",
            DefectKind::InconsistentWinding => "Flipped face",
            DefectKind::DegenerateFace => "Degenerate",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DefectLine {
    pub kind: DefectKind,
    pub a: [f32; 3],
    pub b: [f32; 3],
}

#[derive(Clone, Copy, Debug)]
pub struct DefectMarker {
    pub kind: DefectKind,
    pub position: [f32; 3],
    pub count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Watertight,
    Warning,
    Failed,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Verdict::Watertight => "Watertight",
            Verdict::Warning => "Needs a look",
            Verdict::Failed => "Not watertight",
        }
    }
}

#[derive(Clone, Debug)]
pub struct MeshReport {
    pub triangles: usize,
    pub vertices: usize,
    pub open_edges: usize,
    pub non_manifold_edges: usize,
    pub inconsistent_edges: usize,
    pub degenerate_triangles: usize,
    pub invalid_triangles: usize,
    pub shells: usize,
    pub volume_mm3: f64,
    pub signed_volume_mm3: f64,
    pub surface_area_mm2: f64,
    pub bounds: Bounds,
    pub markers: Vec<DefectMarker>,
    pub defect_lines: Vec<DefectLine>,
    pub check_ms: f64,
}

impl Default for MeshReport {
    fn default() -> Self {
        Self {
            triangles: 0,
            vertices: 0,
            open_edges: 0,
            non_manifold_edges: 0,
            inconsistent_edges: 0,
            degenerate_triangles: 0,
            invalid_triangles: 0,
            shells: 0,
            volume_mm3: 0.0,
            signed_volume_mm3: 0.0,
            surface_area_mm2: 0.0,
            bounds: Bounds::empty(),
            markers: Vec::new(),
            defect_lines: Vec::new(),
            check_ms: 0.0,
        }
    }
}

impl MeshReport {
    /// Topology is closed and consistently wound.
    pub fn is_watertight(&self) -> bool {
        self.triangles > 0
            && self.open_edges == 0
            && self.non_manifold_edges == 0
            && self.inconsistent_edges == 0
            && self.invalid_triangles == 0
    }

    pub fn verdict(&self) -> Verdict {
        if self.triangles == 0 || !self.is_watertight() {
            return Verdict::Failed;
        }
        if self.degenerate_triangles > 0 || self.signed_volume_mm3 <= 0.0 {
            return Verdict::Warning;
        }
        Verdict::Watertight
    }

    /// One line a maker can act on.
    pub fn headline(&self) -> String {
        if self.triangles == 0 {
            return "No triangles in this file".into();
        }
        if self.open_edges > 0 {
            return format!(
                "{} open edge{} in {} shell{}",
                n(self.open_edges),
                s(self.open_edges),
                n(self.shells),
                s(self.shells)
            );
        }
        if self.non_manifold_edges > 0 {
            return format!(
                "{} non-manifold edge{}",
                n(self.non_manifold_edges),
                s(self.non_manifold_edges)
            );
        }
        if self.inconsistent_edges > 0 {
            return format!(
                "{} face{} point the wrong way",
                n(self.inconsistent_edges),
                s(self.inconsistent_edges)
            );
        }
        if self.signed_volume_mm3 <= 0.0 {
            return "Normals point inward".into();
        }
        if self.degenerate_triangles > 0 {
            return format!(
                "{} degenerate triangle{}",
                n(self.degenerate_triangles),
                s(self.degenerate_triangles)
            );
        }
        format!("Closed surface, {} shell{}", n(self.shells), s(self.shells))
    }
}

fn n(v: usize) -> String {
    let s = v.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn s(v: usize) -> &'static str {
    if v == 1 { "" } else { "s" }
}

#[derive(Clone, Copy, Default)]
struct EdgeAcc {
    count: u8,
    forward: u8,
    backward: u8,
    tri_a: u32,
    tri_b: u32,
}

const MARKER_EDGE_CAP: usize = 250_000;

pub fn analyze(mesh: &Mesh, weld_positions: bool) -> MeshReport {
    let started = Instant::now();
    let welded;
    let mesh = if weld_positions {
        welded = weld(mesh);
        &welded
    } else {
        mesh
    };

    let mut report = MeshReport {
        triangles: mesh.indices.len(),
        vertices: mesh.positions.len(),
        bounds: mesh.bounds(),
        ..Default::default()
    };
    if mesh.indices.is_empty() || mesh.positions.is_empty() {
        report.check_ms = started.elapsed().as_secs_f64() * 1000.0;
        return report;
    }

    let vertex_count = mesh.positions.len();
    let diag = report.bounds.diagonal().max(1e-6) as f64;
    let degenerate_area = 1e-9_f64;

    // Area, signed volume, and degenerate faces in one parallel sweep.
    let (area, signed_volume, degenerate, invalid) = mesh
        .indices
        .par_iter()
        .map(|t| {
            let (ia, ib, ic) = (t[0] as usize, t[1] as usize, t[2] as usize);
            if ia >= vertex_count || ib >= vertex_count || ic >= vertex_count {
                return (0.0, 0.0, 0usize, 1usize);
            }
            let a = Vec3::from_array(mesh.positions[ia]);
            let b = Vec3::from_array(mesh.positions[ib]);
            let c = Vec3::from_array(mesh.positions[ic]);
            let cross = (b - a).cross(c - a);
            let a2 = cross.length() as f64;
            let vol = a.dot(b.cross(c)) as f64 / 6.0;
            (
                a2 * 0.5,
                vol,
                if a2 * 0.5 <= degenerate_area { 1 } else { 0 },
                0,
            )
        })
        .reduce(
            || (0.0, 0.0, 0usize, 0usize),
            |a, b| (a.0 + b.0, a.1 + b.1, a.2 + b.2, a.3 + b.3),
        );

    report.surface_area_mm2 = area;
    report.signed_volume_mm3 = signed_volume;
    report.volume_mm3 = signed_volume.abs();
    report.degenerate_triangles = degenerate;
    report.invalid_triangles = invalid;

    let mut edges: FxHashMap<u64, EdgeAcc> = FxHashMap::with_capacity_and_hasher(
        mesh.indices.len().saturating_mul(3),
        Default::default(),
    );

    for (tri, t) in mesh.indices.iter().enumerate() {
        let tri = tri as u32;
        let (a, b, c) = (t[0], t[1], t[2]);
        if a as usize >= vertex_count || b as usize >= vertex_count || c as usize >= vertex_count {
            continue;
        }
        for (u, v) in [(a, b), (b, c), (c, a)] {
            let key = if u < v {
                ((u as u64) << 32) | v as u64
            } else {
                ((v as u64) << 32) | u as u64
            };
            let acc = edges.entry(key).or_default();
            acc.count = acc.count.saturating_add(1);
            if u < v {
                acc.forward = acc.forward.saturating_add(1);
            } else {
                acc.backward = acc.backward.saturating_add(1);
            }
            if acc.count == 1 {
                acc.tri_a = tri;
            } else if acc.count == 2 {
                acc.tri_b = tri;
            }
        }
    }

    let mut boundary: Vec<(u32, u32)> = Vec::new();
    let mut non_manifold: Vec<(u32, u32)> = Vec::new();
    let mut flipped: Vec<(u32, u32)> = Vec::new();
    let mut shell_pairs: Vec<(u32, u32)> = Vec::new();

    for (key, acc) in &edges {
        let a = (key >> 32) as u32;
        let b = (*key & 0xffff_ffff) as u32;
        match acc.count {
            1 => {
                report.open_edges += 1;
                if boundary.len() < MARKER_EDGE_CAP {
                    boundary.push((a, b));
                }
            }
            2 => {
                shell_pairs.push((acc.tri_a, acc.tri_b));
                if acc.forward == 1 && acc.backward == 1 {
                    // properly joined
                } else {
                    report.inconsistent_edges += 1;
                    if flipped.len() < MARKER_EDGE_CAP {
                        flipped.push((a, b));
                    }
                }
            }
            _ => {
                report.non_manifold_edges += 1;
                shell_pairs.push((acc.tri_a, acc.tri_b));
                if non_manifold.len() < MARKER_EDGE_CAP {
                    non_manifold.push((a, b));
                }
            }
        }
    }

    let mut uf = UnionFind::new(mesh.indices.len() as u32);
    for (a, b) in &shell_pairs {
        uf.union(*a, *b);
    }
    let mut roots = std::collections::HashSet::with_capacity(64);
    for tri in 0..mesh.indices.len() as u32 {
        roots.insert(uf.find(tri));
    }
    report.shells = roots.len();

    // Marker positions for the viewport, clustered by connectivity.
    let mut markers = Vec::new();
    cluster_edges(
        &boundary,
        vertex_count,
        &mesh.positions,
        DefectKind::OpenEdge,
        &mut markers,
    );
    cluster_edges(
        &non_manifold,
        vertex_count,
        &mesh.positions,
        DefectKind::NonManifold,
        &mut markers,
    );
    cluster_edges(
        &flipped,
        vertex_count,
        &mesh.positions,
        DefectKind::InconsistentWinding,
        &mut markers,
    );
    cluster_centroids(
        mesh,
        degenerate_area,
        diag,
        DefectKind::DegenerateFace,
        &mut markers,
    );
    markers.sort_by_key(|marker| std::cmp::Reverse(marker.count));
    markers.truncate(8);
    report.markers = markers;

    let mut lines = Vec::new();
    push_lines(&boundary, DefectKind::OpenEdge, &mesh.positions, &mut lines);
    push_lines(
        &non_manifold,
        DefectKind::NonManifold,
        &mesh.positions,
        &mut lines,
    );
    push_lines(
        &flipped,
        DefectKind::InconsistentWinding,
        &mesh.positions,
        &mut lines,
    );
    lines.truncate(120_000);
    report.defect_lines = lines;

    report.check_ms = started.elapsed().as_secs_f64() * 1000.0;
    report
}

fn push_lines(
    edges: &[(u32, u32)],
    kind: DefectKind,
    positions: &[[f32; 3]],
    out: &mut Vec<DefectLine>,
) {
    for (a, b) in edges {
        let (a, b) = (*a as usize, *b as usize);
        if a >= positions.len() || b >= positions.len() {
            continue;
        }
        out.push(DefectLine {
            kind,
            a: positions[a],
            b: positions[b],
        });
    }
}

fn cluster_edges(
    edges: &[(u32, u32)],
    vertex_count: usize,
    positions: &[[f32; 3]],
    kind: DefectKind,
    out: &mut Vec<DefectMarker>,
) {
    if edges.is_empty() {
        return;
    }
    let mut uf = UnionFind::new(vertex_count as u32);
    for (a, b) in edges {
        uf.union(*a, *b);
    }
    let mut groups: FxHashMap<u32, (Vec3, u32)> = FxHashMap::default();
    for (a, b) in edges {
        let na = Vec3::from_array(positions[*a as usize]);
        let nb = Vec3::from_array(positions[*b as usize]);
        let entry = groups.entry(uf.find(*a)).or_insert((Vec3::ZERO, 0));
        entry.0 += (na + nb) * 0.5;
        entry.1 += 1;
    }
    let mut list: Vec<(Vec3, u32)> = groups
        .into_values()
        .map(|(sum, count)| (sum / count as f32, count))
        .collect();
    list.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    for (pos, count) in list.into_iter().take(3) {
        out.push(DefectMarker {
            kind,
            position: pos.to_array(),
            count,
        });
    }
}

fn cluster_centroids(
    mesh: &Mesh,
    degenerate_area: f64,
    diag: f64,
    kind: DefectKind,
    out: &mut Vec<DefectMarker>,
) {
    let cell = (diag / 24.0).max(0.5);
    let mut groups: FxHashMap<(i64, i64, i64), (Vec3, u32)> = FxHashMap::default();
    for t in &mesh.indices {
        let (ia, ib, ic) = (t[0] as usize, t[1] as usize, t[2] as usize);
        if ia >= mesh.positions.len() || ib >= mesh.positions.len() || ic >= mesh.positions.len() {
            continue;
        }
        let a = Vec3::from_array(mesh.positions[ia]);
        let b = Vec3::from_array(mesh.positions[ib]);
        let c = Vec3::from_array(mesh.positions[ic]);
        if ((b - a).cross(c - a).length() as f64) * 0.5 > degenerate_area {
            continue;
        }
        let centroid = (a + b + c) / 3.0;
        let key = (
            (centroid.x as f64 / cell).floor() as i64,
            (centroid.y as f64 / cell).floor() as i64,
            (centroid.z as f64 / cell).floor() as i64,
        );
        let entry = groups.entry(key).or_insert((Vec3::ZERO, 0));
        entry.0 += centroid;
        entry.1 += 1;
    }
    let mut list: Vec<(Vec3, u32)> = groups
        .into_values()
        .map(|(sum, count)| (sum / count as f32, count))
        .collect();
    list.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    for (pos, count) in list.into_iter().take(3) {
        out.push(DefectMarker {
            kind,
            position: pos.to_array(),
            count,
        });
    }
}

struct UnionFind {
    parent: Vec<u32>,
}

impl UnionFind {
    fn new(n: u32) -> Self {
        Self {
            parent: (0..n).collect(),
        }
    }

    fn find(&mut self, mut x: u32) -> u32 {
        let mut root = x;
        while self.parent[root as usize] != root {
            root = self.parent[root as usize];
        }
        while self.parent[x as usize] != root {
            let next = self.parent[x as usize];
            self.parent[x as usize] = root;
            x = next;
        }
        root
    }

    fn union(&mut self, a: u32, b: u32) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra != rb {
            self.parent[rb as usize] = ra;
        }
    }
}

/// Aggregate per-object reports into one scene-level report.
pub fn summarize(reports: &[MeshReport]) -> MeshReport {
    let mut out = MeshReport::default();
    for r in reports {
        out.triangles += r.triangles;
        out.vertices += r.vertices;
        out.open_edges += r.open_edges;
        out.non_manifold_edges += r.non_manifold_edges;
        out.inconsistent_edges += r.inconsistent_edges;
        out.degenerate_triangles += r.degenerate_triangles;
        out.invalid_triangles += r.invalid_triangles;
        out.shells += r.shells;
        out.volume_mm3 += r.volume_mm3;
        out.signed_volume_mm3 += r.signed_volume_mm3;
        out.surface_area_mm2 += r.surface_area_mm2;
        out.bounds.union(&r.bounds);
        out.check_ms += r.check_ms;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn closed_tetra_is_watertight() {
        let r = analyze(&tetra(), false);
        assert_eq!(r.open_edges, 0);
        assert_eq!(r.non_manifold_edges, 0);
        assert_eq!(r.inconsistent_edges, 0);
        assert_eq!(r.shells, 1);
        assert!((r.volume_mm3 - 1.0 / 6.0).abs() < 1e-9);
        assert_eq!(r.verdict(), Verdict::Watertight);
    }

    #[test]
    fn missing_face_opens_three_edges() {
        let mut mesh = tetra();
        mesh.indices.pop();
        let r = analyze(&mesh, false);
        assert_eq!(r.open_edges, 3);
        assert_eq!(r.verdict(), Verdict::Failed);
        assert_eq!(r.markers.len(), 1);
        assert_eq!(r.markers[0].kind, DefectKind::OpenEdge);
    }

    #[test]
    fn flipped_face_is_detected() {
        let mut mesh = tetra();
        mesh.indices[0] = [0, 1, 2];
        let r = analyze(&mesh, false);
        assert_eq!(r.open_edges, 0);
        assert_eq!(r.inconsistent_edges, 3);
        assert_eq!(r.verdict(), Verdict::Failed);
    }

    #[test]
    fn welding_makes_expanded_stl_watertight() {
        let mesh = tetra();
        let mut expanded = Mesh::default();
        for t in &mesh.indices {
            let base = expanded.positions.len() as u32;
            expanded.positions.push(mesh.positions[t[0] as usize]);
            expanded.positions.push(mesh.positions[t[1] as usize]);
            expanded.positions.push(mesh.positions[t[2] as usize]);
            expanded.indices.push([base, base + 1, base + 2]);
        }
        let raw = analyze(&expanded, false);
        assert_eq!(raw.open_edges, 12);
        let welded = analyze(&expanded, true);
        assert_eq!(welded.open_edges, 0);
        assert_eq!(welded.shells, 1);
        assert_eq!(welded.verdict(), Verdict::Watertight);
    }

    #[test]
    fn two_shells_are_counted() {
        let mut mesh = tetra();
        mesh.append(
            &tetra(),
            &glam::Mat4::from_translation(glam::Vec3::new(4.0, 0.0, 0.0)),
        );
        let r = analyze(&mesh, false);
        assert_eq!(r.shells, 2);
        assert_eq!(r.open_edges, 0);
        assert!((r.volume_mm3 - 2.0 / 6.0).abs() < 1e-9);
    }

    #[test]
    fn inverted_normals_warn_without_failing_topology() {
        let mut mesh = tetra();
        for t in &mut mesh.indices {
            t.swap(1, 2);
        }
        let r = analyze(&mesh, false);
        assert_eq!(r.open_edges, 0);
        assert_eq!(r.inconsistent_edges, 0);
        assert!(r.signed_volume_mm3 < 0.0);
        assert_eq!(r.verdict(), Verdict::Warning);
    }

    #[test]
    fn summary_aggregates() {
        let r = analyze(&tetra(), false);
        let s = summarize(&[r.clone(), r]);
        assert_eq!(s.triangles, 8);
        assert!((s.volume_mm3 - 1.0 / 3.0).abs() < 1e-9);
        assert_eq!(s.verdict(), Verdict::Watertight);
    }
}
