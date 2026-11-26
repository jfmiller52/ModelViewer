use crate::mesh::{Bounds, Mesh};
use glam::Vec3;
use rustc_hash::FxHashMap;

const QUANTUM: f64 = 1e-4;
const DEGENERATE_AREA: f64 = 1e-9;

type PointKey = (i64, i64, i64);
type EdgeKey = (i64, i64, i64, i64, i64, i64);

#[derive(Clone, Copy, Debug)]
struct EdgeAcc {
    total: u8,
    forward: u8,
    backward: u8,
    first_face: u32,
}

#[derive(Clone, Debug, Default)]
pub struct ScanProgress {
    pub triangles: usize,
    pub total: usize,
    pub open_edges: usize,
    pub non_manifold_edges: usize,
    pub inconsistent_edges: usize,
    pub degenerate_triangles: usize,
    pub shells: usize,
    pub volume_mm3: f64,
    pub surface_area_mm2: f64,
    pub bounds: Bounds,
    pub elapsed_ms: f64,
    pub done: bool,
}

impl ScanProgress {
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            return 0.0;
        }
        (self.triangles as f32 / self.total as f32).clamp(0.0, 1.0)
    }
}

/// A cheap, streaming topology pre-scan for large triangle soups.
/// Counts are provisional until the authoritative analysis completes.
pub struct StreamScan {
    mesh: Mesh,
    edges: FxHashMap<EdgeKey, EdgeAcc>,
    parents: Vec<u32>,
    shells: usize,
    signed_volume: f64,
    surface_area: f64,
    degenerate: usize,
    bounds: Bounds,
    total: usize,
    started: std::time::Instant,
}

impl StreamScan {
    pub fn new(total: usize) -> Self {
        Self {
            mesh: Mesh {
                positions: Vec::with_capacity(total.saturating_mul(3)),
                indices: Vec::with_capacity(total),
            },
            edges: FxHashMap::default(),
            parents: Vec::with_capacity(total),
            shells: 0,
            signed_volume: 0.0,
            surface_area: 0.0,
            degenerate: 0,
            bounds: Bounds::empty(),
            total,
            started: std::time::Instant::now(),
        }
    }

    pub fn push_triangle(&mut self, triangle: [[f32; 3]; 3]) {
        let face = self.mesh.indices.len() as u32;
        let base = self.mesh.positions.len() as u32;
        for point in triangle {
            let point = point.map(|value| if value.is_finite() { value } else { 0.0 });
            self.bounds.add_point(Vec3::from_array(point));
            self.mesh.positions.push(point);
        }
        self.mesh.indices.push([base, base + 1, base + 2]);
        self.parents.push(face);
        self.shells += 1;

        let [a, b, c] = triangle.map(Vec3::from_array);
        let normal = (b - a).cross(c - a);
        let area = normal.length() as f64 * 0.5;
        if area <= DEGENERATE_AREA {
            self.degenerate += 1;
        }
        self.surface_area += area;
        self.signed_volume += a.dot(b.cross(c)) as f64 / 6.0;

        let keys = triangle.map(quantize);
        self.add_edge(keys[0], keys[1], face);
        self.add_edge(keys[1], keys[2], face);
        self.add_edge(keys[2], keys[0], face);
    }

    fn add_edge(&mut self, a: PointKey, b: PointKey, face: u32) {
        let (key, forward) = if a <= b {
            ((a.0, a.1, a.2, b.0, b.1, b.2), true)
        } else {
            ((b.0, b.1, b.2, a.0, a.1, a.2), false)
        };
        let first_face = if let Some(entry) = self.edges.get_mut(&key) {
            entry.total = entry.total.saturating_add(1);
            if forward {
                entry.forward = entry.forward.saturating_add(1);
            } else {
                entry.backward = entry.backward.saturating_add(1);
            }
            Some(entry.first_face)
        } else {
            self.edges.insert(
                key,
                EdgeAcc {
                    total: 1,
                    forward: u8::from(forward),
                    backward: u8::from(!forward),
                    first_face: face,
                },
            );
            None
        };
        if let Some(first_face) = first_face {
            self.union(face, first_face);
        }
    }

    fn find(&mut self, mut index: u32) -> u32 {
        let mut root = index;
        while self.parents[root as usize] != root {
            root = self.parents[root as usize];
        }
        while self.parents[index as usize] != index {
            let next = self.parents[index as usize];
            self.parents[index as usize] = root;
            index = next;
        }
        root
    }

    fn union(&mut self, a: u32, b: u32) {
        let root_a = self.find(a);
        let root_b = self.find(b);
        if root_a != root_b {
            self.parents[root_b as usize] = root_a;
            self.shells = self.shells.saturating_sub(1);
        }
    }

    pub fn progress(&mut self) -> ScanProgress {
        let mut open_edges = 0;
        let mut non_manifold_edges = 0;
        let mut inconsistent_edges = 0;
        for edge in self.edges.values() {
            if edge.total == 1 {
                open_edges += 1;
            }
            if edge.total > 2 {
                non_manifold_edges += 1;
            }
            if edge.total >= 2 && (edge.forward > 1 || edge.backward > 1) {
                inconsistent_edges += 1;
            }
        }
        ScanProgress {
            triangles: self.mesh.indices.len(),
            total: self.total,
            open_edges,
            non_manifold_edges,
            inconsistent_edges,
            degenerate_triangles: self.degenerate,
            shells: self.shells,
            volume_mm3: self.signed_volume.abs(),
            surface_area_mm2: self.surface_area,
            bounds: self.bounds,
            elapsed_ms: self.started.elapsed().as_secs_f64() * 1000.0,
            done: self.total > 0 && self.mesh.indices.len() >= self.total,
        }
    }

    pub fn into_mesh(self) -> Mesh {
        self.mesh
    }
}

fn quantize(point: [f32; 3]) -> PointKey {
    (
        (point[0] as f64 / QUANTUM).round() as i64,
        (point[1] as f64 / QUANTUM).round() as i64,
        (point[2] as f64 / QUANTUM).round() as i64,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tetra(direction: bool) -> [[[f32; 3]; 3]; 4] {
        let mut faces = [
            [[0.0, 0.0, 0.0], [0.0, 20.0, 0.0], [20.0, 0.0, 0.0]],
            [[0.0, 0.0, 0.0], [20.0, 0.0, 0.0], [0.0, 0.0, 20.0]],
            [[20.0, 0.0, 0.0], [0.0, 20.0, 0.0], [0.0, 0.0, 20.0]],
            [[0.0, 20.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 20.0]],
        ];
        if direction {
            faces[0].swap(1, 2);
        }
        faces
    }

    #[test]
    fn scans_a_closed_tetra() {
        let mut scan = StreamScan::new(4);
        for face in tetra(false) {
            scan.push_triangle(face);
        }
        let progress = scan.progress();
        assert_eq!(progress.triangles, 4);
        assert_eq!(progress.open_edges, 0);
        assert_eq!(progress.non_manifold_edges, 0);
        assert_eq!(progress.inconsistent_edges, 0);
        assert_eq!(progress.shells, 1);
        assert!((progress.volume_mm3 - 1_333.333_3).abs() < 0.1);
        assert!((progress.surface_area_mm2 - 946.41).abs() < 0.1);
    }

    #[test]
    fn detects_open_and_degenerate_during_scan() {
        let mut scan = StreamScan::new(3);
        scan.push_triangle([[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]]);
        scan.push_triangle([[10.0, 0.0, 0.0], [0.0, 10.0, 0.0], [10.0, 10.0, 0.0]]);
        scan.push_triangle([[0.0, 0.0, 0.0], [5.0, 0.0, 0.0], [10.0, 0.0, 0.0]]);
        let progress = scan.progress();
        assert_eq!(progress.triangles, 3);
        assert!(progress.open_edges > 0);
        assert_eq!(progress.degenerate_triangles, 1);
    }

    #[test]
    fn detects_inconsistent_winding_during_scan() {
        let mut scan = StreamScan::new(4);
        for face in tetra(true) {
            scan.push_triangle(face);
        }
        let progress = scan.progress();
        assert!(progress.inconsistent_edges > 0);
        assert_eq!(progress.open_edges, 0);
        assert_eq!(progress.shells, 1);
    }
}
