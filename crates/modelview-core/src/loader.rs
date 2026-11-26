use crate::scene::{Format, LengthUnit, Scene, SceneObject};
use crate::threemf::RecoveredScene;
use crate::{stl, threemf};
use anyhow::{Context, Result, bail};
use std::path::Path;

/// Load an STL or 3MF from disk. The file extension is a hint; the bytes decide.
pub fn load_path(path: &Path) -> Result<Scene> {
    let bytes =
        std::fs::read(path).with_context(|| format!("could not read {}", path.display()))?;
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "model".to_string());
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| name.clone());
    let ext = path
        .extension()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    let mut scene = match ext.as_str() {
        "stl" => stl_scene(&bytes, &stem)?,
        "3mf" => threemf::parse(&bytes, &stem, bytes.len() as u64)?,
        _ => sniff(&bytes, &stem)?,
    };
    scene.source_path = Some(path.display().to_string());
    scene.byte_len = bytes.len() as u64;
    Ok(scene)
}

/// Load a large 3MF by resolving external object parts and skipping broken
/// parts instead of failing the whole package. Used by the app's load-recovery
/// offer; never writes the source.
pub fn load_path_recovering(path: &Path) -> Result<RecoveredScene> {
    let bytes =
        std::fs::read(path).with_context(|| format!("could not read {}", path.display()))?;
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "model".to_string());
    if !bytes.starts_with(b"PK\x03\x04") {
        bail!("load recovery is only available for 3MF packages");
    }
    threemf::parse_recovering(&bytes, &stem, bytes.len() as u64)
}

/// Load a model already held in memory (used for the built-in sample).
pub fn load_bytes(bytes: &[u8], name: &str) -> Result<Scene> {
    let mut scene = sniff(bytes, name)?;
    scene.name = name.to_string();
    scene.byte_len = bytes.len() as u64;
    Ok(scene)
}

fn sniff(bytes: &[u8], name: &str) -> Result<Scene> {
    if bytes.starts_with(b"PK\x03\x04") {
        return threemf::parse(bytes, name, bytes.len() as u64);
    }
    if bytes.len() >= 84 || bytes.starts_with(b"solid") {
        return stl_scene(bytes, name);
    }
    bail!("unrecognised file: expected an STL or 3MF")
}

fn stl_scene(bytes: &[u8], name: &str) -> Result<Scene> {
    let mesh = stl::parse(bytes)?;
    if mesh.is_empty() {
        bail!("this STL contains no triangles");
    }
    Ok(Scene {
        name: name.to_string(),
        source_path: None,
        format: Format::Stl,
        unit: LengthUnit::Millimeter,
        assumed_mm: true,
        objects: vec![SceneObject {
            name: "Mesh".to_string(),
            mesh,
            plate: None,
            source_id: None,
            color: None,
            filament: None,
        }],
        plates: Vec::new(),
        project: None,
        warnings: Vec::new(),
        byte_len: bytes.len() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_ascii_stl_without_extension() {
        let bytes = b"solid t\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid t\n";
        let scene = sniff(bytes, "t").unwrap();
        assert_eq!(scene.format, Format::Stl);
        assert_eq!(scene.objects.len(), 1);
        assert!(scene.assumed_mm);
    }

    #[test]
    fn rejects_garbage() {
        assert!(sniff(b"hello", "x").is_err());
    }
}
