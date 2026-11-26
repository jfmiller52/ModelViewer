use anyhow::{Context, Result};
use modelview_core::{Format, Scene, read_project_settings, write_3mf, write_stl};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchFile {
    pub path: PathBuf,
    pub temporary: bool,
    pub repaired: bool,
}

pub struct LaunchInput<'a> {
    pub scene: &'a Scene,
    pub source_path: Option<&'a Path>,
    pub repaired: bool,
    pub saved_repaired_path: Option<&'a Path>,
}

pub fn prepare_launch_file(input: LaunchInput<'_>) -> Result<LaunchFile> {
    if let Some(saved) = input.saved_repaired_path
        && saved.exists()
    {
        return Ok(LaunchFile {
            path: saved.to_path_buf(),
            temporary: false,
            repaired: input.repaired,
        });
    }
    if !input.repaired
        && let Some(source) = input.source_path
        && source.exists()
    {
        return Ok(LaunchFile {
            path: source.to_path_buf(),
            temporary: false,
            repaired: false,
        });
    }

    let directory = std::env::temp_dir().join("ModelView");
    std::fs::create_dir_all(&directory)
        .with_context(|| format!("could not create {}", directory.display()))?;
    let extension = match input.scene.format {
        Format::Stl => "stl",
        Format::ThreeMf => "3mf",
    };
    let stem = input
        .source_path
        .and_then(|path| path.file_stem())
        .map(|stem| stem.to_string_lossy().to_string())
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| {
            if input.scene.name.is_empty() {
                "model".to_string()
            } else {
                input.scene.name.clone()
            }
        });
    let path = directory.join(format!(
        "{stem}-modelview-{}-{}.{extension}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));

    match input.scene.format {
        Format::Stl => {
            let object = input
                .scene
                .objects
                .first()
                .context("STL scene has no object to export")?;
            write_stl(&path, &object.mesh)?;
        }
        Format::ThreeMf => {
            let project_settings = input
                .source_path
                .filter(|path| {
                    path.exists()
                        && path
                            .extension()
                            .map(|ext| ext.eq_ignore_ascii_case("3mf"))
                            .unwrap_or(false)
                })
                .and_then(|path| read_project_settings(path).ok())
                .flatten();
            write_3mf(&path, input.scene, project_settings.as_deref())?;
        }
    }

    Ok(LaunchFile {
        path,
        temporary: true,
        repaired: input.repaired,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use modelview_core::{LengthUnit, Plate, SceneObject, analyze, stl, threemf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_NEXT: AtomicU64 = AtomicU64::new(1);

    fn temp_file(ext: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "modelview-working-{}-{}.{ext}",
            std::process::id(),
            TEST_NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, b"").unwrap();
        path
    }

    fn tetra() -> modelview_core::Mesh {
        modelview_core::Mesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            indices: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        }
    }

    fn stl_scene() -> Scene {
        Scene {
            name: "tetra".into(),
            source_path: None,
            format: Format::Stl,
            unit: LengthUnit::Millimeter,
            assumed_mm: true,
            objects: vec![SceneObject {
                name: "Mesh".into(),
                mesh: tetra(),
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

    fn three_mf_scene() -> Scene {
        Scene {
            name: "tetra".into(),
            source_path: None,
            format: Format::ThreeMf,
            unit: LengthUnit::Millimeter,
            assumed_mm: false,
            objects: vec![SceneObject {
                name: "Mesh".into(),
                mesh: tetra(),
                plate: Some(1),
                source_id: Some(1),
                color: None,
                filament: None,
            }],
            plates: vec![Plate {
                id: 1,
                name: "Plate 01".into(),
                objects: vec![0],
            }],
            project: None,
            warnings: vec![],
            byte_len: 0,
        }
    }

    #[test]
    fn keeps_the_original_when_untouched_and_present() {
        let source = temp_file("stl");
        let scene = stl_scene();
        let file = prepare_launch_file(LaunchInput {
            scene: &scene,
            source_path: Some(&source),
            repaired: false,
            saved_repaired_path: None,
        })
        .unwrap();
        assert_eq!(file.path, source);
        assert!(!file.temporary);
        let _ = std::fs::remove_file(source);
    }

    #[test]
    fn missing_original_exports_a_temporary_copy() {
        let missing = std::env::temp_dir().join("definitely-missing-modelview.stl");
        let scene = stl_scene();
        let file = prepare_launch_file(LaunchInput {
            scene: &scene,
            source_path: Some(&missing),
            repaired: false,
            saved_repaired_path: None,
        })
        .unwrap();
        assert!(file.path.exists());
        assert!(file.temporary);
        assert_eq!(
            stl::parse(&std::fs::read(&file.path).unwrap())
                .unwrap()
                .triangle_count(),
            4
        );
        let _ = std::fs::remove_file(file.path);
    }

    #[test]
    fn missing_3mf_original_exports_without_project_settings() {
        let missing = std::env::temp_dir().join("definitely-missing-modelview.3mf");
        let scene = three_mf_scene();
        let file = prepare_launch_file(LaunchInput {
            scene: &scene,
            source_path: Some(&missing),
            repaired: false,
            saved_repaired_path: None,
        })
        .unwrap();
        assert!(file.temporary);
        let bytes = std::fs::read(&file.path).unwrap();
        let back = threemf::parse(&bytes, "repaired", bytes.len() as u64).unwrap();
        assert_eq!(back.objects.len(), scene.objects.len());
        assert!(read_project_settings(&file.path).unwrap().is_none());
        let _ = std::fs::remove_file(file.path);
    }

    #[test]
    fn repaired_3mf_writes_a_parseable_temporary_copy() {
        let scene = three_mf_scene();
        let file = prepare_launch_file(LaunchInput {
            scene: &scene,
            source_path: None,
            repaired: true,
            saved_repaired_path: None,
        })
        .unwrap();
        assert!(file.path.exists());
        assert!(file.temporary);
        let bytes = std::fs::read(&file.path).unwrap();
        let back = threemf::parse(&bytes, "repaired", bytes.len() as u64).unwrap();
        assert_eq!(back.objects.len(), scene.objects.len());
        for object in &back.objects {
            assert_eq!(analyze(&object.mesh, false).open_edges, 0);
        }
        let _ = std::fs::remove_file(file.path);
    }

    #[test]
    fn saved_repaired_copy_wins_over_a_temp_export() {
        let scene = stl_scene();
        let saved = temp_file("stl");
        let file = prepare_launch_file(LaunchInput {
            scene: &scene,
            source_path: None,
            repaired: true,
            saved_repaired_path: Some(&saved),
        })
        .unwrap();
        assert_eq!(file.path, saved);
        assert!(file.repaired);
        assert!(!file.temporary);
        let _ = std::fs::remove_file(saved);
    }
}
