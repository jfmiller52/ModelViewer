use crate::mesh::Mesh;
use crate::scene::Scene;
use anyhow::{Context, Result};
use glam::Vec3;
use std::fmt::Write as FmtWrite;
use std::io::{BufWriter, Cursor, Read, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/>
  <Default Extension="config" ContentType="application/octet-stream"/>
</Types>"#;

const RELS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Target="/3D/3dmodel.model" Id="rel0" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/>
</Relationships>"#;

pub fn write_stl(path: &Path, mesh: &Mesh) -> Result<()> {
    let file = std::fs::File::create(path)
        .with_context(|| format!("could not create {}", path.display()))?;
    let mut file = BufWriter::new(file);
    file.write_all(&[0u8; 80])?;
    file.write_all(&(mesh.indices.len() as u32).to_le_bytes())?;
    for t in &mesh.indices {
        let a = Vec3::from_array(mesh.positions[t[0] as usize]);
        let b = Vec3::from_array(mesh.positions[t[1] as usize]);
        let c = Vec3::from_array(mesh.positions[t[2] as usize]);
        let normal = (b - a).cross(c - a).normalize_or_zero();
        for value in [
            normal.x, normal.y, normal.z, a.x, a.y, a.z, b.x, b.y, b.z, c.x, c.y, c.z,
        ] {
            file.write_all(&value.to_le_bytes())?;
        }
        file.write_all(&0u16.to_le_bytes())?;
    }
    file.flush()?;
    Ok(())
}

pub fn write_3mf(path: &Path, scene: &Scene, project_settings: Option<&str>) -> Result<()> {
    let project_settings = sanitize_project_settings(scene, project_settings);
    let file = std::fs::File::create(path)
        .with_context(|| format!("could not create {}", path.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("[Content_Types].xml", options)?;
    zip.write_all(CONTENT_TYPES.as_bytes())?;
    zip.start_file("_rels/.rels", options)?;
    zip.write_all(RELS.as_bytes())?;
    zip.start_file("3D/3dmodel.model", options)?;
    write_model_xml(&mut zip, scene)?;
    if let Some(settings) = project_settings.as_deref() {
        zip.start_file("Metadata/project_settings.config", options)?;
        zip.write_all(settings.as_bytes())?;
    }
    if !scene.plates.is_empty() {
        zip.start_file("Metadata/model_settings.config", options)?;
        zip.write_all(model_settings_xml(scene).as_bytes())?;
    }
    zip.finish()?;
    Ok(())
}

fn sanitize_project_settings(scene: &Scene, settings: Option<&str>) -> Option<String> {
    let settings = settings?;
    match serde_json::from_str::<serde_json::Value>(settings) {
        Ok(value) if has_object_references(&value) => minimal_project_settings(scene),
        _ => Some(settings.to_string()),
    }
}

fn has_object_references(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(map) => map.iter().any(|(key, value)| {
            let key = key.to_ascii_lowercase();
            key.contains("object_id")
                || key.contains("plate_id")
                || key == "object"
                || key == "objects"
                || key == "plate"
                || key == "plates"
                || has_object_references(value)
        }),
        serde_json::Value::Array(values) => values.iter().any(has_object_references),
        _ => false,
    }
}

fn minimal_project_settings(scene: &Scene) -> Option<String> {
    let project = scene.project.as_ref()?;
    let mut map = serde_json::Map::new();
    if let Some(printer) = &project.printer {
        map.insert(
            "printer_model".to_string(),
            serde_json::Value::String(printer.clone()),
        );
    }
    if !project.filaments.is_empty() {
        map.insert(
            "filament_type".to_string(),
            serde_json::Value::Array(
                project
                    .filaments
                    .iter()
                    .cloned()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
    }
    if map.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(map).to_string())
    }
}

pub fn read_project_settings(path: &Path) -> Result<Option<String>> {
    let bytes =
        std::fs::read(path).with_context(|| format!("could not read {}", path.display()))?;
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).context("not a 3MF package")?;
    match archive.by_name("Metadata/project_settings.config") {
        Ok(mut file) => {
            let mut text = String::new();
            file.read_to_string(&mut text)?;
            Ok(Some(text))
        }
        Err(zip::result::ZipError::FileNotFound) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn write_model_xml<W: Write>(xml: &mut W, scene: &Scene) -> Result<()> {
    write!(
        xml,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xml:lang="en-US" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
  <metadata name="Application">ModelView</metadata>
  <resources>
"#
    )?;
    for (index, object) in scene.objects.iter().enumerate() {
        let id = index + 1;
        write!(
            xml,
            "    <object id=\"{id}\" type=\"model\" name=\"{}\">\n      <mesh>\n        <vertices>\n",
            escape_xml(&object.name)
        )?;
        for p in &object.mesh.positions {
            writeln!(
                xml,
                "          <vertex x=\"{:.6}\" y=\"{:.6}\" z=\"{:.6}\"/>",
                p[0], p[1], p[2]
            )?;
        }
        write!(xml, "        </vertices>\n        <triangles>\n")?;
        for t in &object.mesh.indices {
            writeln!(
                xml,
                "          <triangle v1=\"{}\" v2=\"{}\" v3=\"{}\"/>",
                t[0], t[1], t[2]
            )?;
        }
        write!(xml, "        </triangles>\n      </mesh>\n    </object>\n")?;
    }
    write!(xml, "  </resources>\n  <build>\n")?;
    for index in 0..scene.objects.len() {
        writeln!(xml, "    <item objectid=\"{}\" />", index + 1)?;
    }
    write!(xml, "  </build>\n</model>\n")?;
    Ok(())
}

fn model_settings_xml(scene: &Scene) -> String {
    let mut xml = String::new();
    let _ = write!(
        xml,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<config>\n"
    );
    for (index, object) in scene.objects.iter().enumerate() {
        let _ = write!(
            xml,
            "  <object id=\"{}\"><metadata key=\"name\" value=\"{}\"/>",
            index + 1,
            escape_xml(&object.name)
        );
        if let Some(plate) = object.plate {
            let _ = write!(xml, "<metadata key=\"plater_id\" value=\"{plate}\"/>");
        }
        if let Some(filament) = object.filament {
            let _ = write!(xml, "<metadata key=\"extruder\" value=\"{filament}\"/>");
        }
        let _ = writeln!(xml, "</object>");
    }
    for plate in &scene.plates {
        let _ = writeln!(
            xml,
            "  <plate><metadata key=\"plater_id\" value=\"{}\"/><metadata key=\"plater_name\" value=\"{}\"/>",
            plate.id,
            escape_xml(&plate.name)
        );
        for object_index in &plate.objects {
            let _ = writeln!(
                xml,
                "    <model_instance><metadata key=\"object_id\" value=\"{}\"/><metadata key=\"instance_id\" value=\"0\"/></model_instance>",
                object_index + 1
            );
        }
        let _ = writeln!(xml, "  </plate>");
    }
    let _ = writeln!(xml, "</config>");
    xml
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::analyze;
    use crate::scene::{Format, LengthUnit, Plate, SceneObject};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(1);

    fn temp_path(ext: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "modelview-export-{}-{}.{ext}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

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

    fn two_object_scene_with_plates() -> Scene {
        Scene {
            name: "two".into(),
            source_path: None,
            format: Format::ThreeMf,
            unit: LengthUnit::Millimeter,
            assumed_mm: false,
            objects: vec![
                SceneObject {
                    name: "broken".into(),
                    mesh: tetra(),
                    plate: Some(1),
                    source_id: Some(7),
                    color: None,
                    filament: None,
                },
                SceneObject {
                    name: "clean & true".into(),
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
        }
    }

    #[test]
    fn repaired_multi_object_project_round_trips_plates_and_settings() {
        let mut scene = two_object_scene_with_plates();
        scene.objects[0].mesh.indices.pop();
        let (repaired, stats) =
            crate::repair::repair_scene(&scene, crate::repair::RepairOptions::default());
        assert_eq!(stats.len(), 2);
        assert!(stats[0].holes_filled > 0);
        let path = temp_path("3mf");
        let settings = r#"{"printer_model":"Bambu Lab X1 Carbon","filament_type":["PLA"]}"#;
        write_3mf(&path, &repaired, Some(settings)).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let back = crate::threemf::parse(&bytes, "repaired", bytes.len() as u64).unwrap();
        assert_eq!(back.objects.len(), 2);
        assert_eq!(back.objects[0].name, "broken");
        assert_eq!(back.objects[1].name, "clean & true");
        assert_eq!(back.plates.len(), 2);
        assert!(back.plates.iter().any(|p| p.name == "Plate 01"));
        assert_eq!(
            back.project.as_ref().and_then(|p| p.printer.as_deref()),
            Some("Bambu Lab X1 Carbon")
        );
        assert_eq!(
            read_project_settings(&path).unwrap().as_deref(),
            Some(settings)
        );
        for object in &back.objects {
            let report = analyze(&object.mesh, false);
            assert_eq!(report.open_edges, 0);
            assert_eq!(report.inconsistent_edges, 0);
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn stale_object_references_fall_back_to_minimal_project_settings() {
        let mut scene = two_object_scene_with_plates();
        scene.project = Some(crate::scene::ProjectInfo {
            application: None,
            printer: Some("Bambu Lab P1S".into()),
            filaments: vec!["PETG".into()],
        });
        let stale = r#"{"printer_model":"old","plate_id":1,"objects":[{"object_id":7}]}"#;
        let path = temp_path("3mf");
        write_3mf(&path, &scene, Some(stale)).unwrap();
        let text = read_project_settings(&path).unwrap().unwrap();
        assert!(!text.contains("object_id"));
        assert!(!text.contains("plate_id"));
        assert!(text.contains("Bambu Lab P1S"));
        assert!(text.contains("PETG"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn stl_round_trip_is_watertight() {
        let mesh = tetra();
        let path = temp_path("stl");
        write_stl(&path, &mesh).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let back = crate::stl::parse(&bytes).unwrap();
        assert_eq!(back.triangle_count(), mesh.triangle_count());
        let report = analyze(&crate::mesh::weld(&back), false);
        assert_eq!(report.open_edges, 0);
        assert_eq!(report.inconsistent_edges, 0);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn three_mf_round_trip_preserves_objects_names_plates_and_settings() {
        let scene = two_object_scene_with_plates();
        let path = temp_path("3mf");
        let settings = r#"{"printer_model":"Bambu Lab X1 Carbon","filament_type":["PLA"]}"#;
        write_3mf(&path, &scene, Some(settings)).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let back = crate::threemf::parse(&bytes, "repaired", bytes.len() as u64).unwrap();
        assert_eq!(back.objects.len(), 2);
        assert_eq!(back.objects[0].name, "broken");
        assert_eq!(back.objects[1].name, "clean & true");
        assert_eq!(back.plates.len(), 2);
        assert!(back.plates.iter().any(|p| p.name == "Plate 01"));
        assert_eq!(
            back.project.as_ref().and_then(|p| p.printer.as_deref()),
            Some("Bambu Lab X1 Carbon")
        );
        assert_eq!(
            read_project_settings(&path).unwrap().as_deref(),
            Some(settings)
        );
        for object in &back.objects {
            assert_eq!(analyze(&object.mesh, false).open_edges, 0);
        }
        let _ = std::fs::remove_file(path);
    }
}
