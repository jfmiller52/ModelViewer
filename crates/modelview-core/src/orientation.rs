use crate::scene::Scene;
use glam::{Quat, Vec3};
use rayon::prelude::*;
use rustc_hash::FxHashMap;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bed {
    pub width: f32,
    pub depth: f32,
    pub height: f32,
}

impl Default for Bed {
    fn default() -> Self {
        Self {
            width: 256.0,
            depth: 256.0,
            height: 256.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct OrientationCandidate {
    pub name: String,
    pub rotation: [f32; 4],
    pub translation: [f32; 3],
    pub size_mm: [f32; 3],
    pub height_mm: f32,
    pub footprint_mm: [f32; 2],
    pub contact_mm2: f64,
    pub overhang_mm2: f64,
    pub fits_bed: bool,
    pub recommended: bool,
    pub lowest: bool,
    pub most_contact: bool,
    pub score: f32,
}

#[derive(Clone, Debug)]
pub struct OrientationReport {
    pub bed: Bed,
    pub candidates: Vec<OrientationCandidate>,
    pub fits_any: bool,
}

#[derive(Clone, Copy)]
struct RawCandidate {
    rotation: Quat,
    translation: Vec3,
    size: Vec3,
    contact: f64,
    overhang: f64,
    score: f32,
}

pub fn advise_orientation(scene: &Scene, bed: Bed) -> OrientationReport {
    let rotations = candidate_rotations(scene);
    let mut raw: Vec<RawCandidate> = rotations
        .into_par_iter()
        .filter_map(|rotation| evaluate(scene, rotation))
        .collect();

    raw.retain(|candidate| candidate.size.is_finite());
    if raw.is_empty() {
        return OrientationReport {
            bed,
            candidates: Vec::new(),
            fits_any: false,
        };
    }

    let fits = |candidate: &RawCandidate| {
        candidate.size.x <= bed.width + 0.01
            && candidate.size.y <= bed.depth + 0.01
            && candidate.size.z <= bed.height + 0.01
    };
    let fits_any = raw.iter().any(fits);
    let (min_height, max_height, min_overhang, max_overhang, max_contact) =
        raw.iter()
            .fold((f32::MAX, 0.0_f32, f64::MAX, 0.0_f64, 0.0_f64), |acc, c| {
                (
                    acc.0.min(c.size.z),
                    acc.1.max(c.size.z),
                    acc.2.min(c.overhang),
                    acc.3.max(c.overhang),
                    acc.4.max(c.contact),
                )
            });
    for candidate in &mut raw {
        let fit = fits(candidate);
        let height_n = if max_height > min_height {
            (candidate.size.z - min_height) / (max_height - min_height)
        } else {
            0.0
        };
        let overhang_n = if max_overhang > min_overhang + 1e-9 {
            ((candidate.overhang - min_overhang) / (max_overhang - min_overhang)) as f32
        } else {
            0.0
        };
        let contact_n = if max_contact > 1e-9 {
            (candidate.contact / max_contact) as f32
        } else {
            0.0
        };
        let overflow = (candidate.size.x - bed.width)
            .max(candidate.size.y - bed.depth)
            .max(candidate.size.z - bed.height)
            .max(0.0);
        candidate.score = if fit {
            height_n * 0.45 + overhang_n * 0.45 - contact_n * 0.10
        } else {
            1.0 + overflow / bed.width.max(1.0) + height_n * 0.2
        };
    }
    raw.sort_by(|a, b| a.score.total_cmp(&b.score));

    let lowest = raw
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.size.z.total_cmp(&b.size.z))
        .map(|(index, _)| index)
        .unwrap_or(0);
    let most_contact = raw
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| b.contact.total_cmp(&a.contact))
        .map(|(index, _)| index)
        .unwrap_or(0);
    let min_overhang = raw
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.overhang.total_cmp(&b.overhang))
        .map(|(index, _)| index)
        .unwrap_or(0);

    let same_pose = |a: &RawCandidate, b: &RawCandidate| {
        let a_low = a.size.x.min(a.size.y);
        let a_high = a.size.x.max(a.size.y);
        let b_low = b.size.x.min(b.size.y);
        let b_high = b.size.x.max(b.size.y);
        (a.size.z - b.size.z).abs() < 0.01
            && (a_low - b_low).abs() < 0.01
            && (a_high - b_high).abs() < 0.01
            && (a.contact - b.contact).abs() < 0.5
            && (a.overhang - b.overhang).abs() < 0.1
    };
    let mut picks = vec![0usize];
    for index in [lowest, most_contact, min_overhang] {
        if !picks.contains(&index)
            && !picks
                .iter()
                .any(|picked| same_pose(&raw[*picked], &raw[index]))
        {
            picks.push(index);
        }
    }
    picks.truncate(3);

    let mut candidates = Vec::with_capacity(picks.len());
    for (rank, index) in picks.into_iter().enumerate() {
        let raw = raw[index];
        let mut roles = Vec::new();
        if rank == 0 {
            roles.push("RECOMMENDED");
        }
        if index == lowest {
            roles.push("LOWEST PROFILE");
        }
        if index == most_contact {
            roles.push("MOST CONTACT");
        }
        if index == min_overhang {
            roles.push("FEWEST OVERHANGS");
        }
        if roles.is_empty() {
            roles.push("POSE");
        }
        candidates.push(OrientationCandidate {
            name: roles.join(" / "),
            rotation: raw.rotation.to_array(),
            translation: raw.translation.to_array(),
            size_mm: raw.size.to_array(),
            height_mm: raw.size.z,
            footprint_mm: [raw.size.x, raw.size.y],
            contact_mm2: raw.contact,
            overhang_mm2: raw.overhang,
            fits_bed: raw.size.x <= bed.width + 0.01
                && raw.size.y <= bed.depth + 0.01
                && raw.size.z <= bed.height + 0.01,
            recommended: rank == 0,
            lowest: index == lowest,
            most_contact: index == most_contact,
            score: raw.score,
        });
    }
    OrientationReport {
        bed,
        candidates,
        fits_any,
    }
}

fn evaluate(scene: &Scene, rotation: Quat) -> Option<RawCandidate> {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for object in &scene.objects {
        for point in &object.mesh.positions {
            let point = rotation * Vec3::from_array(*point);
            if !point.is_finite() {
                return None;
            }
            min = min.min(point);
            max = max.max(point);
        }
    }
    if !min.is_finite() || !max.is_finite() {
        return None;
    }
    let size = max - min;
    let translation = Vec3::new(-(min.x + max.x) * 0.5, -(min.y + max.y) * 0.5, -min.z);
    let bottom_tolerance = (size.z * 0.01).max(0.05);
    let mut contact = 0.0;
    let mut overhang = 0.0;
    for object in &scene.objects {
        let positions = &object.mesh.positions;
        for triangle in &object.mesh.indices {
            let Some(a) = positions.get(triangle[0] as usize) else {
                continue;
            };
            let Some(b) = positions.get(triangle[1] as usize) else {
                continue;
            };
            let Some(c) = positions.get(triangle[2] as usize) else {
                continue;
            };
            let a = rotation * Vec3::from_array(*a);
            let b = rotation * Vec3::from_array(*b);
            let c = rotation * Vec3::from_array(*c);
            let cross = (b - a).cross(c - a);
            let area = cross.length() as f64 * 0.5;
            if area <= 1e-9 {
                continue;
            }
            let normal = cross.normalize();
            let lowest = a.z.min(b.z).min(c.z);
            if normal.z < -0.995 && lowest <= min.z + bottom_tolerance {
                contact += area;
            } else if normal.z < -0.707_106_77 {
                overhang += area;
            }
        }
    }
    Some(RawCandidate {
        rotation,
        translation,
        size,
        contact,
        overhang,
        score: 0.0,
    })
}

fn candidate_rotations(scene: &Scene) -> Vec<Quat> {
    let mut normals: FxHashMap<(i16, i16, i16), (Vec3, f64)> = FxHashMap::default();
    for object in &scene.objects {
        let positions = &object.mesh.positions;
        for triangle in &object.mesh.indices {
            let Some(a) = positions.get(triangle[0] as usize) else {
                continue;
            };
            let Some(b) = positions.get(triangle[1] as usize) else {
                continue;
            };
            let Some(c) = positions.get(triangle[2] as usize) else {
                continue;
            };
            let cross = (Vec3::from_array(*b) - Vec3::from_array(*a))
                .cross(Vec3::from_array(*c) - Vec3::from_array(*a));
            let area = cross.length() as f64 * 0.5;
            if area <= 1e-9 || !cross.is_finite() {
                continue;
            }
            let normal = cross.normalize();
            let key = (
                (normal.x * 32.0).round() as i16,
                (normal.y * 32.0).round() as i16,
                (normal.z * 32.0).round() as i16,
            );
            let entry = normals.entry(key).or_insert((normal, 0.0));
            entry.1 += area;
        }
    }
    let mut normals: Vec<(Vec3, f64)> = normals.into_values().collect();
    normals.sort_by(|a, b| b.1.total_cmp(&a.1));
    normals.truncate(8);
    let mut downs = vec![Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z];
    downs.extend(normals.into_iter().map(|(normal, _)| normal));
    let mut rotations = Vec::new();
    for down in downs {
        let align = Quat::from_rotation_arc(down.normalize(), Vec3::NEG_Z);
        for quarter in 0..4 {
            let rotation =
                Quat::from_rotation_z(quarter as f32 * std::f32::consts::FRAC_PI_2) * align;
            if rotations
                .iter()
                .all(|other: &Quat| other.dot(rotation).abs() < 0.999_99)
            {
                rotations.push(rotation);
            }
        }
    }
    rotations
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Format, LengthUnit, Mesh, SceneObject};

    fn box_scene(size: Vec3) -> Scene {
        let h = size * 0.5;
        let positions = vec![
            [-h.x, -h.y, -h.z],
            [h.x, -h.y, -h.z],
            [h.x, h.y, -h.z],
            [-h.x, h.y, -h.z],
            [-h.x, -h.y, h.z],
            [h.x, -h.y, h.z],
            [h.x, h.y, h.z],
            [-h.x, h.y, h.z],
        ];
        let indices = vec![
            [0, 2, 1],
            [0, 3, 2],
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
        Scene {
            name: "box".into(),
            source_path: None,
            format: Format::Stl,
            unit: LengthUnit::Millimeter,
            assumed_mm: true,
            objects: vec![SceneObject {
                name: "Mesh".into(),
                mesh: Mesh { positions, indices },
                plate: None,
                source_id: None,
                color: None,
                filament: None,
            }],
            plates: Vec::new(),
            project: None,
            warnings: Vec::new(),
            byte_len: 0,
        }
    }

    #[test]
    fn recommends_a_fitting_low_box_pose() {
        let report = advise_orientation(&box_scene(Vec3::new(40.0, 20.0, 10.0)), Bed::default());
        assert!(report.fits_any);
        let best = &report.candidates[0];
        assert!(best.fits_bed);
        assert!((best.height_mm - 10.0).abs() < 0.1);
        assert!(best.contact_mm2 > 100.0);
        assert!(report.candidates.iter().any(|c| c.lowest));
    }

    #[test]
    fn reports_no_fit_for_a_too_small_bed() {
        let report = advise_orientation(
            &box_scene(Vec3::new(40.0, 20.0, 10.0)),
            Bed {
                width: 15.0,
                depth: 15.0,
                height: 15.0,
            },
        );
        assert!(!report.fits_any);
        assert!(!report.candidates.is_empty());
        assert!(report.candidates.iter().all(|c| !c.fits_bed));
    }
}
