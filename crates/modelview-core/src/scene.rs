use crate::mesh::{Bounds, Mesh};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Stl,
    ThreeMf,
}

impl Format {
    pub fn label(self) -> &'static str {
        match self {
            Format::Stl => "STL",
            Format::ThreeMf => "3MF",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LengthUnit {
    Micron,
    Millimeter,
    Centimeter,
    Inch,
    Foot,
    Meter,
}

impl LengthUnit {
    pub fn mm(self) -> f32 {
        match self {
            LengthUnit::Micron => 0.001,
            LengthUnit::Millimeter => 1.0,
            LengthUnit::Centimeter => 10.0,
            LengthUnit::Inch => 25.4,
            LengthUnit::Foot => 304.8,
            LengthUnit::Meter => 1000.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            LengthUnit::Micron => "micron",
            LengthUnit::Millimeter => "millimetre",
            LengthUnit::Centimeter => "centimetre",
            LengthUnit::Inch => "inch",
            LengthUnit::Foot => "foot",
            LengthUnit::Meter => "metre",
        }
    }

    pub fn parse(value: Option<&str>) -> Self {
        match value.unwrap_or("millimeter").to_ascii_lowercase().as_str() {
            "micron" => LengthUnit::Micron,
            "centimeter" => LengthUnit::Centimeter,
            "inch" => LengthUnit::Inch,
            "foot" => LengthUnit::Foot,
            "meter" => LengthUnit::Meter,
            _ => LengthUnit::Millimeter,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SceneObject {
    pub name: String,
    pub mesh: Mesh,
    pub plate: Option<u32>,
    pub source_id: Option<u32>,
    pub color: Option<[u8; 3]>,
    pub filament: Option<u8>,
}

#[derive(Clone, Debug)]
pub struct Plate {
    pub id: u32,
    pub name: String,
    pub objects: Vec<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct ProjectInfo {
    pub application: Option<String>,
    pub printer: Option<String>,
    pub filaments: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Scene {
    pub name: String,
    pub source_path: Option<String>,
    pub format: Format,
    pub unit: LengthUnit,
    /// STL carries no unit; the viewer assumes millimetres and says so.
    pub assumed_mm: bool,
    pub objects: Vec<SceneObject>,
    pub plates: Vec<Plate>,
    pub project: Option<ProjectInfo>,
    pub warnings: Vec<String>,
    pub byte_len: u64,
}

impl Scene {
    pub fn bounds(&self) -> Bounds {
        let mut b = Bounds::empty();
        for o in &self.objects {
            b.union(&o.mesh.bounds());
        }
        b
    }

    pub fn triangles(&self) -> usize {
        self.objects.iter().map(|o| o.mesh.triangle_count()).sum()
    }

    pub fn vertices(&self) -> usize {
        self.objects.iter().map(|o| o.mesh.vertex_count()).sum()
    }

    pub fn empty() -> Self {
        Self {
            name: String::new(),
            source_path: None,
            format: Format::Stl,
            unit: LengthUnit::Millimeter,
            assumed_mm: true,
            objects: Vec::new(),
            plates: Vec::new(),
            project: None,
            warnings: Vec::new(),
            byte_len: 0,
        }
    }
}
