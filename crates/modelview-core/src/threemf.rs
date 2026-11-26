use crate::mesh::Mesh;
use crate::scene::{Format, LengthUnit, Plate, ProjectInfo, Scene, SceneObject};
use anyhow::{Context, Result, bail};
use glam::{Mat4, Vec3};
use rustc_hash::FxHashMap;
use std::io::{Cursor, Read};
use zip::ZipArchive;

const PRODUCTION_NS: &str = "http://schemas.microsoft.com/3dmanufacturing/production/2015/06";

struct Component {
    id: u32,
    transform: Mat4,
    path: Option<String>,
}

struct ObjectDef {
    name: String,
    mesh: Option<Mesh>,
    components: Vec<Component>,
}

/// Parse a 3MF package into a flattened, millimetre-space scene.
pub fn parse(bytes: &[u8], file_name: &str, byte_len: u64) -> Result<Scene> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .context("this does not look like a 3MF package (zip archive)")?;

    let model_path = find_model_path(&mut archive)?;
    let xml = read_entry(&mut archive, &model_path)?
        .with_context(|| format!("3MF package is missing its model part ({model_path})"))?;
    let doc = roxmltree::Document::parse(&xml).context("3MF model is not valid XML")?;
    let root = doc.root_element();
    if !root.has_tag_name("model") {
        bail!("3MF model part has no <model> element");
    }

    let unit = LengthUnit::parse(root.attribute("unit"));
    let application = root
        .children()
        .find(|n| n.has_tag_name("metadata") && n.attribute("name") == Some("Application"))
        .and_then(|n| n.text())
        .map(|s| s.trim().to_string());

    let mut defs: FxHashMap<u32, ObjectDef> = FxHashMap::default();
    if let Some(resources) = root.children().find(|n| n.has_tag_name("resources")) {
        for object in resources.children().filter(|n| n.has_tag_name("object")) {
            let Some(id) = object.attribute("id").and_then(|v| v.parse::<u32>().ok()) else {
                continue;
            };
            let name = object
                .attribute("name")
                .map(|s| s.to_string())
                .or_else(|| {
                    object
                        .children()
                        .find(|n| n.has_tag_name("metadata") && n.attribute("name") == Some("name"))
                        .and_then(|n| n.attribute("value"))
                        .map(|s| s.to_string())
                })
                .unwrap_or_else(|| format!("Object {id}"));

            let mut components = Vec::new();
            if let Some(group) = object.children().find(|n| n.has_tag_name("components")) {
                for c in group.children().filter(|n| n.has_tag_name("component")) {
                    if let Some(cid) = c.attribute("objectid").and_then(|v| v.parse::<u32>().ok()) {
                        components.push(Component {
                            id: cid,
                            transform: parse_transform(c.attribute("transform")),
                            path: c.attribute((PRODUCTION_NS, "path")).map(str::to_string),
                        });
                    }
                }
            }

            defs.insert(
                id,
                ObjectDef {
                    name,
                    mesh: parse_mesh(object),
                    components,
                },
            );
        }
    }

    let mut items: Vec<(u32, Mat4)> = Vec::new();
    if let Some(build) = root.children().find(|n| n.has_tag_name("build")) {
        for item in build.children().filter(|n| n.has_tag_name("item")) {
            if let Some(id) = item
                .attribute("objectid")
                .and_then(|v| v.parse::<u32>().ok())
            {
                items.push((id, parse_transform(item.attribute("transform"))));
            }
        }
    }

    let mut warnings = Vec::new();
    if items.is_empty() {
        warnings.push("No build items; showing every object in the package.".to_string());
        for (id, def) in &defs {
            if def.mesh.is_some() || !def.components.is_empty() {
                items.push((*id, Mat4::IDENTITY));
            }
        }
    }

    let scale = Mat4::from_scale(Vec3::splat(unit.mm()));
    let mut objects = Vec::with_capacity(items.len());
    let mut name_counts: FxHashMap<String, u32> = FxHashMap::default();
    for (id, transform) in items {
        let mut stack = Vec::new();
        let base = compose(id, &defs, 0, &mut stack)?;
        if base.is_empty() {
            continue;
        }
        let mesh = base.transformed(&(scale * transform));
        let base_name = defs
            .get(&id)
            .map(|d| d.name.clone())
            .unwrap_or_else(|| format!("Object {id}"));
        let count = name_counts.entry(base_name.clone()).or_insert(0);
        *count += 1;
        let name = if *count > 1 {
            format!("{base_name} {}", *count)
        } else {
            base_name
        };
        objects.push(SceneObject {
            name,
            mesh,
            plate: None,
            source_id: Some(id),
            color: None,
            filament: None,
        });
    }

    if objects.is_empty() {
        bail!("3MF package contains no printable objects");
    }

    let mut plates = Vec::new();
    let project_settings = read_entry(&mut archive, "Metadata/project_settings.config")?;
    if let Some(config) = read_entry(&mut archive, "Metadata/model_settings.config")? {
        apply_model_settings_metadata(&mut objects, &config, project_settings.as_deref());
        plates = parse_model_settings(&config, &mut objects).unwrap_or_default();
    }

    let project = read_project_info(&mut archive, application);

    Ok(Scene {
        name: file_name.to_string(),
        source_path: None,
        format: Format::ThreeMf,
        unit,
        assumed_mm: false,
        objects,
        plates,
        project,
        warnings,
        byte_len,
    })
}

fn compose(
    id: u32,
    defs: &FxHashMap<u32, ObjectDef>,
    depth: usize,
    stack: &mut Vec<u32>,
) -> Result<Mesh> {
    if depth > 32 {
        bail!("3MF object graph is nested more than 32 levels");
    }
    if stack.contains(&id) {
        bail!("3MF object graph contains a cycle at object {id}");
    }
    let def = defs
        .get(&id)
        .with_context(|| format!("3MF references a missing object ({id})"))?;
    stack.push(id);
    let mut out = Mesh::default();
    if let Some(mesh) = &def.mesh {
        out.append(mesh, &Mat4::IDENTITY);
    }
    for child in &def.components {
        if let Some(path) = &child.path {
            bail!("3MF references an external object part ({path}); recovery is required");
        }
        let child_mesh = compose(child.id, defs, depth + 1, stack)?;
        out.append(&child_mesh, &child.transform);
    }
    stack.pop();
    Ok(out)
}

fn parse_mesh(object: roxmltree::Node<'_, '_>) -> Option<Mesh> {
    let mesh_node = object.children().find(|n| n.has_tag_name("mesh"))?;
    let vertices_node = mesh_node.children().find(|n| n.has_tag_name("vertices"))?;
    let mut positions = Vec::new();
    for v in vertices_node
        .children()
        .filter(|n| n.has_tag_name("vertex"))
    {
        let x = v.attribute("x").and_then(|s| s.parse::<f32>().ok());
        let y = v.attribute("y").and_then(|s| s.parse::<f32>().ok());
        let z = v.attribute("z").and_then(|s| s.parse::<f32>().ok());
        if let (Some(x), Some(y), Some(z)) = (x, y, z) {
            positions.push([x, y, z]);
        }
    }
    let triangles_node = mesh_node.children().find(|n| n.has_tag_name("triangles"))?;
    let mut indices = Vec::new();
    for t in triangles_node
        .children()
        .filter(|n| n.has_tag_name("triangle"))
    {
        let a = t
            .attribute("v1")
            .or_else(|| t.attribute("p1"))
            .and_then(|s| s.parse::<u32>().ok());
        let b = t
            .attribute("v2")
            .or_else(|| t.attribute("p2"))
            .and_then(|s| s.parse::<u32>().ok());
        let c = t
            .attribute("v3")
            .or_else(|| t.attribute("p3"))
            .and_then(|s| s.parse::<u32>().ok());
        if let (Some(a), Some(b), Some(c)) = (a, b, c)
            && (a as usize) < positions.len()
            && (b as usize) < positions.len()
            && (c as usize) < positions.len()
        {
            indices.push([a, b, c]);
        }
    }
    Some(Mesh { positions, indices })
}

/// 3MF stores a 4x3 row-vector affine transform as twelve numbers.
fn parse_transform(value: Option<&str>) -> Mat4 {
    let Some(value) = value else {
        return Mat4::IDENTITY;
    };
    let values: Vec<f32> = value
        .split_whitespace()
        .filter_map(|v| v.parse::<f32>().ok())
        .collect();
    if values.len() != 12 {
        return Mat4::IDENTITY;
    }
    Mat4::from_cols_array(&[
        values[0], values[1], values[2], 0.0, // column 0
        values[3], values[4], values[5], 0.0, // column 1
        values[6], values[7], values[8], 0.0, // column 2
        values[9], values[10], values[11], 1.0, // translation
    ])
}

fn find_model_path(archive: &mut ZipArchive<Cursor<&[u8]>>) -> Result<String> {
    if let Some(rels) = read_entry(archive, "_rels/.rels")?
        && let Ok(doc) = roxmltree::Document::parse(&rels)
    {
        for rel in doc.descendants().filter(|n| n.has_tag_name("Relationship")) {
            let kind = rel.attribute("Type").unwrap_or_default();
            if kind.ends_with("/3dmodel")
                && let Some(target) = rel.attribute("Target")
            {
                return Ok(target.trim_start_matches('/').to_string());
            }
        }
    }
    if archive.by_name("3D/3dmodel.model").is_ok() {
        return Ok("3D/3dmodel.model".to_string());
    }
    let candidate = archive
        .file_names()
        .find(|n| n.ends_with(".model"))
        .map(|s| s.to_string());
    candidate.context("3MF package has no 3D model part")
}

fn read_entry(archive: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<Option<String>> {
    match archive.by_name(name) {
        Ok(mut file) => {
            let mut text = String::new();
            file.read_to_string(&mut text)
                .with_context(|| format!("could not read {name} inside the 3MF package"))?;
            Ok(Some(text))
        }
        Err(zip::result::ZipError::FileNotFound) => Ok(None),
        Err(error) => {
            Err(error).with_context(|| format!("could not open {name} in the 3MF package"))
        }
    }
}

fn parse_model_settings(config: &str, objects: &mut [SceneObject]) -> Result<Vec<Plate>> {
    let doc = roxmltree::Document::parse(config)?;
    let mut plates: Vec<Plate> = Vec::new();
    let mut object_plates: FxHashMap<u32, u32> = FxHashMap::default();

    for object in doc.descendants().filter(|n| n.has_tag_name("object")) {
        let Some(id) = object.attribute("id").and_then(|v| v.parse::<u32>().ok()) else {
            continue;
        };
        for meta in object.children().filter(|n| n.has_tag_name("metadata")) {
            if matches!(meta.attribute("key"), Some("plater_id") | Some("plate_id"))
                && let Some(plate) = meta.attribute("value").and_then(|v| v.parse::<u32>().ok())
            {
                object_plates.insert(id, plate);
            }
        }
    }

    for plate_node in doc.descendants().filter(|n| n.has_tag_name("plate")) {
        let mut id = None;
        let mut name = None;
        for meta in plate_node.children().filter(|n| n.has_tag_name("metadata")) {
            match meta.attribute("key") {
                Some("plater_id") | Some("plate_id") => {
                    id = meta.attribute("value").and_then(|v| v.parse::<u32>().ok());
                }
                Some("plater_name") | Some("plate_name") => {
                    name = meta.attribute("value").map(|s| s.to_string());
                }
                _ => {}
            }
        }
        let Some(id) = id else { continue };
        for instance in plate_node
            .children()
            .filter(|n| n.has_tag_name("model_instance"))
        {
            for meta in instance.children().filter(|n| n.has_tag_name("metadata")) {
                if meta.attribute("key") == Some("object_id")
                    && let Some(oid) = meta.attribute("value").and_then(|v| v.parse::<u32>().ok())
                {
                    object_plates.entry(oid).or_insert(id);
                }
            }
        }
        plates.push(Plate {
            id,
            name: name.unwrap_or_else(|| format!("Plate {id}")),
            objects: Vec::new(),
        });
    }

    for (index, object) in objects.iter_mut().enumerate() {
        if let Some(source) = object.source_id
            && let Some(plate) = object_plates.get(&source).copied()
        {
            object.plate = Some(plate);
            if let Some(entry) = plates.iter_mut().find(|p| p.id == plate) {
                entry.objects.push(index);
            } else {
                plates.push(Plate {
                    id: plate,
                    name: format!("Plate {plate}"),
                    objects: vec![index],
                });
            }
        }
    }

    plates.sort_by_key(|p| p.id);
    Ok(plates)
}

fn read_project_info(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    application: Option<String>,
) -> Option<ProjectInfo> {
    let mut info = ProjectInfo {
        application,
        ..Default::default()
    };
    if let Ok(Some(text)) = read_entry(archive, "Metadata/project_settings.config")
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&text)
    {
        info.printer = value.get("printer_model").and_then(first_string);
        info.filaments = value
            .get("filament_type")
            .and_then(|v| match v {
                serde_json::Value::Array(items) => Some(
                    items
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect(),
                ),
                serde_json::Value::String(s) => Some(vec![s.clone()]),
                _ => None,
            })
            .unwrap_or_default();
    }
    if info.application.is_none() && info.printer.is_none() && info.filaments.is_empty() {
        None
    } else {
        Some(info)
    }
}

fn first_string(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Array(items) => {
            items.iter().find_map(|v| v.as_str()).map(|s| s.to_string())
        }
        _ => None,
    }
}

#[derive(Clone, Debug, Default)]
pub struct RecoveryReport {
    pub external_parts: usize,
    pub loaded_objects: usize,
    pub skipped_objects: usize,
    pub skipped_components: usize,
    pub triangles: usize,
}

pub struct RecoveredScene {
    pub scene: Scene,
    pub report: RecoveryReport,
}

/// Recovery parse for packages that keep object geometry in external model
/// parts. Broken or missing parts are skipped and reported instead of aborting
/// the whole package; plates and object names from model_settings are kept.
pub fn parse_recovering(bytes: &[u8], file_name: &str, byte_len: u64) -> Result<RecoveredScene> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .context("this does not look like a 3MF package (zip archive)")?;
    let model_path = find_model_path(&mut archive)?;
    let xml = read_entry(&mut archive, &model_path)?
        .with_context(|| format!("3MF package is missing its model part ({model_path})"))?;
    let doc = roxmltree::Document::parse(&xml).context("3MF model is not valid XML")?;
    let root = doc.root_element();
    if !root.has_tag_name("model") {
        bail!("3MF model part has no <model> element");
    }
    let unit = LengthUnit::parse(root.attribute("unit"));
    let application = root
        .children()
        .find(|n| n.has_tag_name("metadata") && n.attribute("name") == Some("Application"))
        .and_then(|n| n.text())
        .map(|s| s.trim().to_string());

    let defs = parse_defs(root.children().find(|n| n.has_tag_name("resources")));
    let mut items: Vec<(u32, Mat4)> = Vec::new();
    if let Some(build) = root.children().find(|n| n.has_tag_name("build")) {
        for item in build.children().filter(|n| n.has_tag_name("item")) {
            if let Some(id) = item
                .attribute("objectid")
                .and_then(|v| v.parse::<u32>().ok())
            {
                items.push((id, parse_transform(item.attribute("transform"))));
            }
        }
    }
    if items.is_empty() {
        for id in defs.keys().copied() {
            items.push((id, Mat4::IDENTITY));
        }
    }

    let settings = read_entry(&mut archive, "Metadata/model_settings.config")?;
    let names = settings
        .as_deref()
        .map(parse_object_names)
        .unwrap_or_default();
    let scale = Mat4::from_scale(Vec3::splat(unit.mm()));
    let mut report = RecoveryReport::default();
    let mut objects = Vec::with_capacity(items.len());
    let mut name_counts: FxHashMap<String, u32> = FxHashMap::default();
    for (id, transform) in items {
        let mut stack = Vec::new();
        let Some(mesh) =
            compose_recovering(&mut archive, &defs, None, id, 0, &mut stack, &mut report)
        else {
            report.skipped_objects += 1;
            continue;
        };
        if mesh.is_empty() {
            report.skipped_objects += 1;
            continue;
        }
        let mesh = mesh.transformed(&(scale * transform));
        let base_name = names
            .get(&id)
            .cloned()
            .or_else(|| defs.get(&id).map(|def| def.name.clone()))
            .unwrap_or_else(|| format!("Object {id}"));
        let count = name_counts.entry(base_name.clone()).or_insert(0);
        *count += 1;
        let name = if *count > 1 {
            format!("{base_name} {}", *count)
        } else {
            base_name
        };
        report.triangles += mesh.triangle_count();
        report.loaded_objects += 1;
        objects.push(SceneObject {
            name,
            mesh,
            plate: None,
            source_id: Some(id),
            color: None,
            filament: None,
        });
    }
    if objects.is_empty() {
        bail!("3MF recovery found no printable objects");
    }

    let mut plates = Vec::new();
    let project_settings = read_entry(&mut archive, "Metadata/project_settings.config")?;
    if let Some(config) = &settings {
        apply_model_settings_metadata(&mut objects, config, project_settings.as_deref());
        plates = parse_model_settings(config, &mut objects).unwrap_or_default();
    }
    let project = read_project_info(&mut archive, application);
    let mut warnings = Vec::new();
    if report.external_parts > 0 {
        warnings.push(format!(
            "Resolved {} external object part{} on load.",
            report.external_parts,
            if report.external_parts == 1 { "" } else { "s" }
        ));
    }
    if report.skipped_objects > 0 || report.skipped_components > 0 {
        warnings.push(format!(
            "Skipped {} broken object{} while recovering the package.",
            report.skipped_objects,
            if report.skipped_objects == 1 { "" } else { "s" }
        ));
    }
    Ok(RecoveredScene {
        scene: Scene {
            name: file_name.to_string(),
            source_path: None,
            format: Format::ThreeMf,
            unit,
            assumed_mm: false,
            objects,
            plates,
            project,
            warnings,
            byte_len,
        },
        report,
    })
}

fn apply_model_settings_metadata(
    objects: &mut [SceneObject],
    config: &str,
    project_settings: Option<&str>,
) {
    let colors = read_filament_colors(project_settings);
    let Ok(doc) = roxmltree::Document::parse(config) else {
        return;
    };
    for node in doc.descendants().filter(|n| n.has_tag_name("object")) {
        let Some(id) = node.attribute("id").and_then(|v| v.parse::<u32>().ok()) else {
            continue;
        };
        let mut name = None;
        let mut extruder = None;
        for meta in node.children().filter(|n| n.has_tag_name("metadata")) {
            match meta.attribute("key") {
                Some("name") => name = meta.attribute("value").map(str::to_string),
                Some("extruder") => {
                    extruder = meta.attribute("value").and_then(|v| v.parse::<u32>().ok());
                }
                _ => {}
            }
        }
        let Some(object) = objects
            .iter_mut()
            .find(|object| object.source_id == Some(id))
        else {
            continue;
        };
        if let Some(name) = name {
            object.name = name;
        }
        if let Some(extruder) = extruder {
            object.filament = Some(extruder.min(u8::MAX as u32) as u8);
            let index = extruder.saturating_sub(1) as usize;
            object.color = colors.get(index).copied();
        }
    }
}

fn read_filament_colors(project_settings: Option<&str>) -> Vec<[u8; 3]> {
    let Some(settings) = project_settings else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(settings) else {
        return Vec::new();
    };
    let Some(items) = value.get("filament_colour").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|value| value.as_str().and_then(parse_hex_color))
        .collect()
}

fn parse_hex_color(value: &str) -> Option<[u8; 3]> {
    let value = value.trim().trim_start_matches('#');
    if value.len() != 6 && value.len() != 8 {
        return None;
    }
    let r = u8::from_str_radix(&value[0..2], 16).ok()?;
    let g = u8::from_str_radix(&value[2..4], 16).ok()?;
    let b = u8::from_str_radix(&value[4..6], 16).ok()?;
    Some([r, g, b])
}

fn parse_defs(resources: Option<roxmltree::Node<'_, '_>>) -> FxHashMap<u32, ObjectDef> {
    let mut defs = FxHashMap::default();
    let Some(resources) = resources else {
        return defs;
    };
    for object in resources.children().filter(|n| n.has_tag_name("object")) {
        let Some(id) = object.attribute("id").and_then(|v| v.parse::<u32>().ok()) else {
            continue;
        };
        let name = object
            .attribute("name")
            .map(str::to_string)
            .or_else(|| {
                object
                    .children()
                    .find(|n| n.has_tag_name("metadata") && n.attribute("name") == Some("name"))
                    .and_then(|n| n.attribute("value"))
                    .map(str::to_string)
            })
            .unwrap_or_else(|| format!("Object {id}"));
        let mut components = Vec::new();
        if let Some(group) = object.children().find(|n| n.has_tag_name("components")) {
            for c in group.children().filter(|n| n.has_tag_name("component")) {
                if let Some(cid) = c.attribute("objectid").and_then(|v| v.parse::<u32>().ok()) {
                    components.push(Component {
                        id: cid,
                        transform: parse_transform(c.attribute("transform")),
                        path: c.attribute((PRODUCTION_NS, "path")).map(str::to_string),
                    });
                }
            }
        }
        defs.insert(
            id,
            ObjectDef {
                name,
                mesh: parse_mesh(object),
                components,
            },
        );
    }
    defs
}

fn parse_object_names(config: &str) -> FxHashMap<u32, String> {
    let mut names = FxHashMap::default();
    let Ok(doc) = roxmltree::Document::parse(config) else {
        return names;
    };
    for object in doc.descendants().filter(|n| n.has_tag_name("object")) {
        let Some(id) = object.attribute("id").and_then(|v| v.parse::<u32>().ok()) else {
            continue;
        };
        for meta in object.children().filter(|n| n.has_tag_name("metadata")) {
            if meta.attribute("key") == Some("name")
                && let Some(value) = meta.attribute("value")
            {
                names.insert(id, value.to_string());
            }
        }
    }
    names
}

fn compose_recovering(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    defs: &FxHashMap<u32, ObjectDef>,
    part_path: Option<&str>,
    id: u32,
    depth: usize,
    stack: &mut Vec<(Option<String>, u32)>,
    report: &mut RecoveryReport,
) -> Option<Mesh> {
    if depth > 32 {
        report.skipped_components += 1;
        return None;
    }
    let stack_key = (part_path.map(str::to_string), id);
    if stack.contains(&stack_key) {
        report.skipped_components += 1;
        return None;
    }
    let Some(def) = defs.get(&id) else {
        report.skipped_components += 1;
        return None;
    };
    stack.push(stack_key);
    let mut out = Mesh::default();
    if let Some(mesh) = &def.mesh {
        out.append(mesh, &Mat4::IDENTITY);
    }
    for child in &def.components {
        let (child_defs, child_path);
        let external;
        if let Some(path) = &child.path {
            let normalized = normalize_part_path(path);
            let Some(xml) = read_entry(archive, &normalized).ok().flatten() else {
                report.skipped_components += 1;
                continue;
            };
            let Ok(doc) = roxmltree::Document::parse(&xml) else {
                report.skipped_components += 1;
                continue;
            };
            external = parse_defs(
                doc.root_element()
                    .children()
                    .find(|n| n.has_tag_name("resources")),
            );
            report.external_parts += 1;
            child_defs = &external;
            child_path = Some(normalized);
        } else {
            child_defs = defs;
            child_path = part_path.map(str::to_string);
        }
        if let Some(child_mesh) = compose_recovering(
            archive,
            child_defs,
            child_path.as_deref(),
            child.id,
            depth + 1,
            stack,
            report,
        ) {
            out.append(&child_mesh, &child.transform);
        }
    }
    stack.pop();
    Some(out)
}

fn normalize_part_path(path: &str) -> String {
    path.trim_start_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn package_with(model: &str) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut out);
            let opts =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            zip.start_file("3D/3dmodel.model", opts).unwrap();
            zip.write_all(model.as_bytes()).unwrap();
            zip.finish().unwrap();
        }
        out.into_inner()
    }

    fn package_with_entries(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut out);
            let opts =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            for (name, body) in entries {
                zip.start_file(*name, opts).unwrap();
                zip.write_all(body.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        out.into_inner()
    }

    fn package() -> Vec<u8> {
        package_with(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
  <metadata name="Application">UnitTest</metadata>
  <resources>
    <object id="1" type="model" name="Triangle">
      <mesh>
        <vertices>
          <vertex x="0" y="0" z="0"/><vertex x="10" y="0" z="0"/><vertex x="0" y="10" z="0"/>
        </vertices>
        <triangles><triangle v1="0" v2="1" v3="2"/></triangles>
      </mesh>
    </object>
  </resources>
  <build><item objectid="1" transform="1 0 0 0 1 0 0 0 1 5 0 0"/></build>
</model>"#,
        )
    }

    #[test]
    fn recovery_resolves_external_object_parts_and_plates() {
        let root = r#"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02" xmlns:p="http://schemas.microsoft.com/3dmanufacturing/production/2015/06">
 <resources>
  <object id="1" type="model"><components><component p:path="/3D/Objects/part.model" objectid="7" transform="1 0 0 0 1 0 0 0 1 10 0 0"/></components></object>
 </resources>
 <build><item objectid="1"/></build>
</model>"#;
        let part = r#"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
 <resources><object id="7" type="model"><mesh>
  <vertices><vertex x="0" y="0" z="0"/><vertex x="2" y="0" z="0"/><vertex x="0" y="2" z="0"/></vertices>
  <triangles><triangle v1="0" v2="1" v3="2"/></triangles>
 </mesh></object></resources>
</model>"#;
        let settings = r#"<config>
 <object id="1"><metadata key="name" value="Imported Wing"/><metadata key="extruder" value="1"/></object>
 <plate><metadata key="plater_id" value="1"/><metadata key="plater_name" value="Plate A"/><model_instance><metadata key="object_id" value="1"/></model_instance></plate>
</config>"#;
        let project = r##"{"filament_colour":["#112233"]}"##;
        let package = package_with_entries(&[
            ("3D/3dmodel.model", root),
            ("3D/Objects/part.model", part),
            ("Metadata/model_settings.config", settings),
            ("Metadata/project_settings.config", project),
        ]);
        let recovered = parse_recovering(&package, "pelican.3mf", package.len() as u64).unwrap();
        assert_eq!(recovered.report.external_parts, 1);
        assert_eq!(recovered.report.loaded_objects, 1);
        assert_eq!(recovered.scene.objects.len(), 1);
        assert_eq!(recovered.scene.objects[0].name, "Imported Wing");
        assert_eq!(recovered.scene.objects[0].color, Some([0x11, 0x22, 0x33]));
        assert_eq!(recovered.scene.objects[0].filament, Some(1));
        assert_eq!(recovered.scene.objects[0].mesh.triangle_count(), 1);
        assert_eq!(recovered.scene.plates.len(), 1);
        assert_eq!(recovered.scene.plates[0].name, "Plate A");
        assert_eq!(recovered.scene.plates[0].objects, vec![0]);
    }

    #[test]
    fn standard_parser_rejects_external_parts() {
        let root = r#"<model xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02" xmlns:p="http://schemas.microsoft.com/3dmanufacturing/production/2015/06">
 <resources><object id="1" type="model"><components><component p:path="/3D/Objects/part.model" objectid="7"/></components></object></resources>
 <build><item objectid="1"/></build></model>"#;
        let package = package_with(root);
        let error = parse(&package, "external.3mf", package.len() as u64).unwrap_err();
        assert!(format!("{error:#}").contains("external object part"));
    }

    #[test]
    fn applies_model_settings_part_colors_on_standard_parse() {
        let root = r#"<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
 <resources><object id="1" type="model"><mesh>
  <vertices><vertex x="0" y="0" z="0"/><vertex x="2" y="0" z="0"/><vertex x="0" y="2" z="0"/></vertices>
  <triangles><triangle v1="0" v2="1" v3="2"/></triangles>
 </mesh></object></resources>
 <build><item objectid="1"/></build></model>"#;
        let settings = r#"<config><object id="1"><metadata key="name" value="Green Part"/><metadata key="extruder" value="1"/></object></config>"#;
        let project = r##"{"filament_colour":["#5E6344"]}"##;
        let package = package_with_entries(&[
            ("3D/3dmodel.model", root),
            ("Metadata/model_settings.config", settings),
            ("Metadata/project_settings.config", project),
        ]);
        let scene = parse(&package, "colored.3mf", package.len() as u64).unwrap();
        assert_eq!(scene.objects[0].name, "Green Part");
        assert_eq!(scene.objects[0].color, Some([0x5e, 0x63, 0x44]));
        assert_eq!(scene.objects[0].filament, Some(1));
    }

    #[test]
    fn parses_transformed_3mf() {
        let scene = parse(&package(), "test", 0).unwrap();
        assert_eq!(scene.objects.len(), 1);
        assert_eq!(scene.objects[0].mesh.triangle_count(), 1);
        // Transform translates +5 mm on X.
        assert!((scene.objects[0].mesh.positions[0][0] - 5.0).abs() < 1e-5);
    }

    #[test]
    fn applies_3mf_units() {
        let xml = package();
        let mut archive = ZipArchive::new(Cursor::new(xml.as_slice())).unwrap();
        assert_eq!(find_model_path(&mut archive).unwrap(), "3D/3dmodel.model");
    }

    #[test]
    fn converts_inch_models_to_millimetres() {
        let bytes = package_with(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<model unit="inch" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
  <resources>
    <object id="1" type="model" name="Inch triangle">
      <mesh>
        <vertices><vertex x="1" y="0" z="0"/><vertex x="2" y="0" z="0"/><vertex x="1" y="1" z="0"/></vertices>
        <triangles><triangle v1="0" v2="1" v3="2"/></triangles>
      </mesh>
    </object>
  </resources>
  <build><item objectid="1"/></build>
</model>"#,
        );
        let scene = parse(&bytes, "inch", 0).unwrap();
        assert!((scene.objects[0].mesh.positions[0][0] - 25.4).abs() < 1e-3);
    }

    #[test]
    fn flattens_component_transforms() {
        let bytes = package_with(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
  <resources>
    <object id="1" type="model" name="Source">
      <mesh>
        <vertices><vertex x="0" y="0" z="0"/><vertex x="4" y="0" z="0"/><vertex x="0" y="4" z="0"/></vertices>
        <triangles><triangle v1="0" v2="1" v3="2"/></triangles>
      </mesh>
    </object>
    <object id="2" type="model" name="Assembly">
      <components><component objectid="1" transform="1 0 0 0 1 0 0 0 1 10 0 0"/></components>
    </object>
  </resources>
  <build><item objectid="2"/></build>
</model>"#,
        );
        let scene = parse(&bytes, "parts", 0).unwrap();
        assert_eq!(scene.objects.len(), 1);
        assert_eq!(scene.objects[0].name, "Assembly");
        assert!((scene.objects[0].mesh.positions[0][0] - 10.0).abs() < 1e-5);
    }
}
