use modelview_core::{
    Format, LengthUnit, MeshReport, ScanProgress, Scene, SceneObject, StreamScan, analyze,
    summarize,
};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

const MIN_TRIANGLES: usize = 100_000;
const MIN_BYTES: u64 = 2_000_000;
const MAX_TRIANGLES: usize = 4_000_000;
const CHUNK_TRIANGLES: usize = 8_192;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(70);

pub struct StreamedModel {
    pub scene: Scene,
    pub reports: Vec<MeshReport>,
    pub summary: MeshReport,
    pub parse_ms: f64,
}

pub struct StreamChunk {
    pub positions: Vec<[f32; 3]>,
    pub scan: ScanProgress,
}

pub enum StreamEvent {
    Chunk(StreamChunk),
    Verifying(ScanProgress),
    Finished(Box<StreamedModel>),
    Failed(String),
}

pub fn progressive_triangle_count(path: &Path) -> Option<usize> {
    let metadata = std::fs::metadata(path).ok()?;
    if metadata.len() < MIN_BYTES {
        return None;
    }
    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("3mf"))
    {
        return None;
    }
    let mut file = std::fs::File::open(path).ok()?;
    let mut header = [0u8; 84];
    file.read_exact(&mut header).ok()?;
    let count = u32::from_le_bytes([header[80], header[81], header[82], header[83]]) as usize;
    if !(MIN_TRIANGLES..=MAX_TRIANGLES).contains(&count) {
        return None;
    }
    let expected = 84usize.checked_add(count.checked_mul(50)?)?;
    if expected > metadata.len() as usize {
        return None;
    }
    Some(count)
}

pub fn spawn_stream(
    path: PathBuf,
    display_name: String,
    tx: Sender<StreamEvent>,
    ctx: egui::Context,
) {
    std::thread::spawn(move || {
        let result = stream_file(&path, &display_name, &tx, &ctx);
        if let Err(error) = result {
            let _ = tx.send(StreamEvent::Failed(error));
            ctx.request_repaint();
        }
    });
}

fn stream_file(
    path: &Path,
    display_name: &str,
    tx: &Sender<StreamEvent>,
    ctx: &egui::Context,
) -> Result<(), String> {
    let started = Instant::now();
    let mut file = std::fs::File::open(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let mut header = [0u8; 84];
    file.read_exact(&mut header)
        .map_err(|_| "truncated binary STL header".to_string())?;
    let total = u32::from_le_bytes([header[80], header[81], header[82], header[83]]) as usize;
    let expected = 84usize
        .checked_add(
            total
                .checked_mul(50)
                .ok_or_else(|| "triangle count overflow".to_string())?,
        )
        .ok_or_else(|| "triangle count overflow".to_string())?;
    let file_len = std::fs::metadata(path)
        .map_err(|error| error.to_string())?
        .len() as usize;
    if expected > file_len {
        return Err(format!(
            "truncated binary STL: header claims {total} triangles but the file is shorter"
        ));
    }

    let mut scan = StreamScan::new(total);
    let mut pending: Vec<[f32; 3]> = Vec::with_capacity(CHUNK_TRIANGLES * 3);
    let mut records = vec![0u8; CHUNK_TRIANGLES * 50];
    let mut seen = 0usize;
    let mut last_progress = ScanProgress {
        total,
        ..ScanProgress::default()
    };
    let mut last_progress_at = started;

    loop {
        let want = (total - seen).min(CHUNK_TRIANGLES) * 50;
        if want == 0 {
            break;
        }
        file.read_exact(&mut records[..want])
            .map_err(|_| "truncated binary STL: the file ended early".to_string())?;
        for record in records[..want].as_chunks::<50>().0 {
            let mut triangle = [[0.0f32; 3]; 3];
            for (index, vertex) in triangle.iter_mut().enumerate() {
                let offset = 12 + index * 12;
                *vertex = [
                    read_f32(record, offset),
                    read_f32(record, offset + 4),
                    read_f32(record, offset + 8),
                ];
            }
            scan.push_triangle(triangle);
            pending.extend_from_slice(&triangle);
        }
        seen += want / 50;
        if last_progress_at.elapsed() >= PROGRESS_INTERVAL {
            last_progress = scan.progress();
            last_progress_at = Instant::now();
        } else {
            last_progress.triangles = seen;
            last_progress.done = seen >= total;
        }
        if tx
            .send(StreamEvent::Chunk(StreamChunk {
                positions: std::mem::take(&mut pending),
                scan: last_progress.clone(),
            }))
            .is_err()
        {
            return Ok(());
        }
        ctx.request_repaint();
    }

    let parse_ms = started.elapsed().as_secs_f64() * 1000.0;
    last_progress = scan.progress();
    last_progress.triangles = total;
    last_progress.done = true;
    let _ = tx.send(StreamEvent::Verifying(last_progress.clone()));
    ctx.request_repaint();

    let mesh = scan.into_mesh();
    let object = SceneObject {
        name: "Mesh".to_string(),
        mesh,
        plate: None,
        source_id: None,
        color: None,
        filament: None,
    };
    let scene = Scene {
        name: display_name.to_string(),
        source_path: Some(path.display().to_string()),
        format: Format::Stl,
        unit: LengthUnit::Millimeter,
        assumed_mm: true,
        objects: vec![object],
        plates: Vec::new(),
        project: None,
        warnings: Vec::new(),
        byte_len: file_len as u64,
    };
    let reports: Vec<MeshReport> = scene
        .objects
        .iter()
        .map(|object| analyze(&object.mesh, true))
        .collect();
    let summary = summarize(&reports);
    let _ = tx.send(StreamEvent::Finished(Box::new(StreamedModel {
        scene,
        reports,
        summary,
        parse_ms,
    })));
    ctx.request_repaint();
    Ok(())
}

fn read_f32(bytes: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}
