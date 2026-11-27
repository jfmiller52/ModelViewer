use glam::{Mat4, Vec3};
use rustc_hash::FxHashMap;

/// Axis-aligned bounds in millimetres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: Vec3,
    pub max: Vec3,
}

impl Default for Bounds {
    fn default() -> Self {
        Self::empty()
    }
}

impl Bounds {
    pub fn empty() -> Self {
        Self {
            min: Vec3::splat(f32::INFINITY),
            max: Vec3::splat(f32::NEG_INFINITY),
        }
    }

    pub fn is_empty(&self) -> bool {
        !(self.min.x <= self.max.x && self.min.y <= self.max.y && self.min.z <= self.max.z)
    }

    pub fn add_point(&mut self, p: Vec3) {
        self.min = self.min.min(p);
        self.max = self.max.max(p);
    }

    pub fn union(&mut self, other: &Bounds) {
        if !other.is_empty() {
            self.add_point(other.min);
            self.add_point(other.max);
        }
    }

    pub fn size(&self) -> Vec3 {
        if self.is_empty() {
            Vec3::ZERO
        } else {
            self.max - self.min
        }
    }

    pub fn center(&self) -> Vec3 {
        if self.is_empty() {
            Vec3::ZERO
        } else {
            (self.min + self.max) * 0.5
        }
    }

    pub fn diagonal(&self) -> f32 {
        self.size().length()
    }

    pub fn radius(&self) -> f32 {
        if self.is_empty() {
            0.0
        } else {
            self.size().length() * 0.5
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<[u32; 3]>,
}

impl Mesh {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty() || self.positions.is_empty()
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len()
    }

    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    pub fn bounds(&self) -> Bounds {
        let mut b = Bounds::empty();
        for p in &self.positions {
            b.add_point(Vec3::from_array(*p));
        }
        b
    }

    /// Append another mesh, transformed into this mesh's space.
    pub fn append(&mut self, other: &Mesh, transform: &Mat4) {
        if other.is_empty() {
            return;
        }
        let base = self.positions.len() as u32;
        if *transform == Mat4::IDENTITY {
            self.positions.extend_from_slice(&other.positions);
        } else {
            self.positions.reserve(other.positions.len());
            for p in &other.positions {
                self.positions
                    .push(transform.transform_point3(Vec3::from_array(*p)).to_array());
            }
        }
        self.indices.reserve(other.indices.len());
        for t in &other.indices {
            self.indices.push([t[0] + base, t[1] + base, t[2] + base]);
        }
    }

    pub fn transformed(&self, transform: &Mat4) -> Mesh {
        let mut out = Mesh::default();
        out.append(self, transform);
        out
    }
}

/// Merge vertices that occupy the same point within a small tolerance.
/// STL has no shared vertex indices, so this is what makes topology visible.
pub fn weld(mesh: &Mesh) -> Mesh {
    if mesh.positions.is_empty() {
        return Mesh::default();
    }
    let bounds = mesh.bounds();
    let tol = (bounds.diagonal() * 1e-6).max(1e-9);
    weld_with_tolerance(mesh, tol)
}

fn weld_with_tolerance(mesh: &Mesh, tol: f32) -> Mesh {
    if mesh.positions.is_empty() {
        return Mesh::default();
    }
    let tol = tol.max(1e-9) as f64;
    let tol_sq = tol * tol;

    let mut local_of = vec![u32::MAX; mesh.positions.len()];
    let mut local_positions: Vec<[f32; 3]> = Vec::with_capacity(mesh.positions.len());
    let mut local_indices: Vec<[u32; 3]> = Vec::with_capacity(mesh.indices.len());
    for t in &mesh.indices {
        let Some(a) = local_index(t[0], mesh, &mut local_of, &mut local_positions) else {
            continue;
        };
        let Some(b) = local_index(t[1], mesh, &mut local_of, &mut local_positions) else {
            continue;
        };
        let Some(c) = local_index(t[2], mesh, &mut local_of, &mut local_positions) else {
            continue;
        };
        local_indices.push([a, b, c]);
    }
    if local_positions.is_empty() {
        return Mesh::default();
    }

    let mut union = UnionFind::new(local_positions.len());
    let mut grid: FxHashMap<(i64, i64, i64), Vec<u32>> = FxHashMap::default();
    for (index, p) in local_positions.iter().copied().enumerate() {
        if !p.iter().all(|value| value.is_finite()) {
            continue;
        }
        let cell = (
            ((p[0] as f64) / tol).floor() as i64,
            ((p[1] as f64) / tol).floor() as i64,
            ((p[2] as f64) / tol).floor() as i64,
        );
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let Some(candidates) = grid.get(&(cell.0 + dx, cell.1 + dy, cell.2 + dz))
                    else {
                        continue;
                    };
                    for &candidate in candidates {
                        let q = local_positions[candidate as usize];
                        let dx = (p[0] as f64) - (q[0] as f64);
                        let dy = (p[1] as f64) - (q[1] as f64);
                        let dz = (p[2] as f64) - (q[2] as f64);
                        if dx * dx + dy * dy + dz * dz <= tol_sq {
                            union.union(index as u32, candidate);
                        }
                    }
                }
            }
        }
        grid.entry(cell).or_default().push(index as u32);
    }

    let mut root_ids: FxHashMap<u32, u32> = FxHashMap::default();
    let mut remap = vec![0u32; local_positions.len()];
    let mut positions = Vec::new();
    for index in 0..local_positions.len() {
        let root = union.find(index as u32);
        let id = *root_ids.entry(root).or_insert_with(|| {
            let id = positions.len() as u32;
            positions.push(local_positions[index]);
            id
        });
        remap[index] = id;
    }
    let indices = local_indices
        .into_iter()
        .map(|t| {
            [
                remap[t[0] as usize],
                remap[t[1] as usize],
                remap[t[2] as usize],
            ]
        })
        .collect();
    Mesh { positions, indices }
}

fn local_index(
    index: u32,
    mesh: &Mesh,
    local_of: &mut [u32],
    positions: &mut Vec<[f32; 3]>,
) -> Option<u32> {
    let slot = local_of.get_mut(index as usize)?;
    if *slot == u32::MAX {
        *slot = positions.len() as u32;
        positions.push(mesh.positions[index as usize]);
    }
    Some(*slot)
}

struct UnionFind {
    parent: Vec<u32>,
}

impl UnionFind {
    fn new(len: usize) -> Self {
        Self {
            parent: (0..len as u32).collect(),
        }
    }

    fn find(&mut self, value: u32) -> u32 {
        let mut root = value as usize;
        while self.parent[root] as usize != root {
            root = self.parent[root] as usize;
        }
        let mut current = value as usize;
        while self.parent[current] as usize != current {
            let next = self.parent[current] as usize;
            self.parent[current] = root as u32;
            current = next;
        }
        root as u32
    }

    fn union(&mut self, a: u32, b: u32) {
        let ar = self.find(a);
        let br = self.find(b);
        if ar != br {
            self.parent[br as usize] = ar;
        }
    }
}

/// Build area-weighted smooth normals for a renderable mesh.
pub fn smooth_normals(positions: &[[f32; 3]], indices: &[[u32; 3]]) -> Vec<[f32; 3]> {
    let mut normals = vec![Vec3::ZERO; positions.len()];
    // Accumulate per-triangle normals; the cross product length is twice the
    // triangle area, so this is already area weighted.
    for t in indices {
        let ia = t[0] as usize;
        let ib = t[1] as usize;
        let ic = t[2] as usize;
        if ia >= positions.len() || ib >= positions.len() || ic >= positions.len() {
            continue;
        }
        let a = Vec3::from_array(positions[ia]);
        let b = Vec3::from_array(positions[ib]);
        let c = Vec3::from_array(positions[ic]);
        let n = (b - a).cross(c - a);
        normals[ia] += n;
        normals[ib] += n;
        normals[ic] += n;
    }

    normals
        .into_iter()
        .map(|n| {
            if n.length_squared() > 1e-24 {
                n.normalize().to_array()
            } else {
                [0.0, 0.0, 1.0]
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn does_not_weld_points_outside_the_tolerance() {
        let mesh = Mesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [0.002, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [1000.0, 0.0, 0.0],
            ],
            indices: vec![[0, 1, 2]],
        };

        let out = weld(&mesh);

        assert_eq!(out.positions.len(), 3);
        assert_eq!(out.indices, vec![[0, 1, 2]]);
    }

    #[test]
    fn welds_points_that_straddle_a_weld_cell_boundary() {
        let mesh = Mesh {
            positions: vec![
                [0.00049, 0.0, 0.0],
                [0.00051, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [1000.0, 0.0, 0.0],
            ],
            indices: vec![[0, 1, 2]],
        };

        let out = weld(&mesh);

        assert_eq!(out.positions.len(), 2);
        assert_eq!(out.indices, vec![[0, 0, 1]]);
    }
}
