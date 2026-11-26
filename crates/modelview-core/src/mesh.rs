use glam::{Mat4, Vec3};

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
    let tol = (bounds.diagonal() * 1e-6).max(1e-6);

    let mut map: rustc_hash::FxHashMap<(i64, i64, i64), u32> =
        rustc_hash::FxHashMap::with_capacity_and_hasher(
            mesh.positions.len().min(8_000_000),
            Default::default(),
        );
    let mut out = Mesh {
        positions: Vec::with_capacity(mesh.positions.len()),
        indices: Vec::with_capacity(mesh.indices.len()),
    };

    let mut remap = |p: [f32; 3]| -> u32 {
        let key = (
            (p[0] as f64 / tol as f64).round() as i64,
            (p[1] as f64 / tol as f64).round() as i64,
            (p[2] as f64 / tol as f64).round() as i64,
        );
        if let Some(id) = map.get(&key) {
            return *id;
        }
        let id = out.positions.len() as u32;
        out.positions.push(p);
        map.insert(key, id);
        id
    };

    for t in &mesh.indices {
        let a = remap(mesh.positions[t[0] as usize]);
        let b = remap(mesh.positions[t[1] as usize]);
        let c = remap(mesh.positions[t[2] as usize]);
        out.indices.push([a, b, c]);
    }
    out
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
