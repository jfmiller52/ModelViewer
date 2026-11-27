use crate::camera::{Camera, ViewPreset};
use crate::gpu::{Focus, LiveRenderer, SceneRenderer};
use crate::repair_flow::{self, RepairPreview, RepairSession};
use crate::settings::Settings;
use crate::slicer::{self, SlicerCandidate, SlicerChoice, SlicerKind};
use crate::streaming::{self, StreamEvent};
use crate::theme;
use crate::working_file::{self, LaunchInput};
use eframe::egui::{
    self, Align, Align2, Button, Color32, CursorIcon, FontId, Id, Key, Layout, Margin,
    PointerButton, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2,
    ViewportCommand,
};
use glam::{Mat4, Quat, Vec3};
use modelview_core::{
    Bed, Bounds, DefectKind, DefectLine, DefectMarker, Format, MeshReport, OrientationReport,
    RecoveryReport, RepairOptions, ScanProgress, Scene, Verdict, advise_orientation, analyze,
    load_path_recovering, read_project_settings, summarize, write_3mf, write_stl,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

const SAMPLE_VASE: &[u8] = include_bytes!("../assets/samples/vase_broken.stl");

pub struct ScreenshotConfig {
    pub path: PathBuf,
    pub delay_frames: u64,
    pub allow_loading: bool,
}

#[derive(Clone, Copy, Debug)]
struct CameraFlight {
    from: Camera,
    to: Camera,
    started: Instant,
    duration: f32,
}

#[derive(Clone, Copy, Debug)]
struct OrientationPose {
    from_rotation: Quat,
    from_translation: Vec3,
    to_rotation: Quat,
    to_translation: Vec3,
    started: Instant,
}

struct OrientationState {
    report: OrientationReport,
    selected: Option<usize>,
}

struct LiveStream {
    started: Instant,
    total: usize,
    scan: ScanProgress,
    bounds: Bounds,
    positions: Vec<[f32; 3]>,
    framed: bool,
    follow: bool,
    verifying: bool,
}

#[derive(Clone, Copy, Debug)]
struct Theater {
    object: usize,
    marker: DefectMarker,
    focus_radius: f32,
    previous_camera: Camera,
    started: Instant,
    closing: Option<Instant>,
}

#[derive(Clone)]
enum Phase {
    Empty,
    Loading { name: String, started: Instant },
    Ready,
    Failed { title: String, message: String },
}

impl Phase {
    fn is_ready(&self) -> bool {
        matches!(self, Phase::Ready)
    }
}

struct LoadedData {
    scene: Scene,
    reports: Vec<MeshReport>,
    summary: MeshReport,
    parse_ms: f64,
    display_name: String,
    recovery: Option<RecoveryReport>,
}

#[derive(Clone)]
struct CheckRow {
    label: &'static str,
    detail: &'static str,
    value: String,
    state: theme::StateMark,
    kind: Option<DefectKind>,
}

pub struct ModelView {
    phase: Phase,
    scene: Option<Scene>,
    stream: Option<LiveStream>,
    stream_rx: Option<Receiver<StreamEvent>>,
    live_gpu: Option<LiveRenderer>,
    live_uploaded: usize,
    scan_display: [f32; 6],
    reports: Vec<MeshReport>,
    summary: MeshReport,
    gpu: Option<SceneRenderer>,
    gpu_defects_dirty: bool,
    gpu_name: String,
    camera: Camera,
    camera_flight: Option<CameraFlight>,
    theater: Option<Theater>,
    orientation: Option<OrientationState>,
    orientation_pose: Option<OrientationPose>,
    orientation_job: Option<Receiver<Result<OrientationReport, String>>>,
    orientation_error: Option<String>,
    orientation_demo: bool,
    selection: Option<usize>,
    visible: Vec<bool>,
    show_grid: bool,
    filters: [bool; 4],
    active_view: Option<ViewPreset>,
    active_section: usize,
    scroll_to: Option<usize>,
    markers: Vec<(usize, DefectMarker)>,
    rx: Option<Receiver<Result<LoadedData, String>>>,
    parse_ms: f64,
    upload_ms: f64,
    last_dir: Option<PathBuf>,
    display_name: String,
    file_path: Option<PathBuf>,
    screenshot: Option<ScreenshotConfig>,
    screenshot_sent: bool,
    viewport_rect: Option<Rect>,
    frames: u64,
    help_open: bool,
    sidebar_open: bool,
    defect_cursor: usize,
    focus_demo: bool,
    theater_demo: bool,
    settings: Settings,
    storage_available: bool,
    settings_open: bool,
    slicer_candidates: Vec<SlicerCandidate>,
    repair: Option<RepairSession>,
    repair_job: Option<Receiver<Result<RepairPreview, String>>>,
    repair_open: bool,
    repair_error: Option<String>,
    repaired: bool,
    repaired_saved_path: Option<PathBuf>,
    recovered: bool,
    recovery_available: bool,
    recovery_load: bool,
    recover_demo: bool,
    notice: Option<(String, Instant)>,
}

impl ModelView {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        initial: Option<PathBuf>,
        screenshot: Option<ScreenshotConfig>,
        help_open: bool,
        inspector_open: bool,
        focus_demo: bool,
        theater_demo: bool,
        orientation_demo: bool,
        recover_demo: bool,
        settings_open: bool,
        repair_open: bool,
    ) -> Self {
        theme::install(&cc.egui_ctx);
        let shot_mode = screenshot.is_some();
        let live_shot = screenshot
            .as_ref()
            .is_some_and(|config| config.allow_loading);
        let has_initial = initial.is_some();
        let storage_available = cc.storage.is_some();
        let settings: Settings = cc
            .storage
            .and_then(|storage| eframe::get_value(storage, eframe::APP_KEY))
            .unwrap_or_default();
        let last_dir = settings
            .last_open_dir
            .clone()
            .or_else(|| std::env::current_dir().ok());
        let slicer_candidates = slicer::detect_slicers();
        let mut app = Self {
            phase: Phase::Empty,
            scene: None,
            stream: None,
            stream_rx: None,
            live_gpu: None,
            live_uploaded: 0,
            scan_display: [0.0; 6],
            reports: Vec::new(),
            summary: MeshReport::default(),
            gpu: None,
            gpu_defects_dirty: false,
            gpu_name: "WGPU".to_string(),
            camera: Camera::default(),
            camera_flight: None,
            theater: None,
            orientation: None,
            orientation_pose: None,
            orientation_job: None,
            orientation_error: None,
            orientation_demo,
            selection: None,
            visible: Vec::new(),
            show_grid: true,
            filters: [true; 4],
            active_view: None,
            active_section: 0,
            scroll_to: None,
            markers: Vec::new(),
            rx: None,
            parse_ms: 0.0,
            upload_ms: 0.0,
            last_dir,
            display_name: String::new(),
            file_path: None,
            screenshot,
            screenshot_sent: false,
            viewport_rect: None,
            frames: 0,
            help_open,
            sidebar_open: inspector_open,
            defect_cursor: 0,
            focus_demo,
            theater_demo,
            settings,
            storage_available,
            settings_open,
            slicer_candidates,
            repair: None,
            repair_job: None,
            repair_open,
            repair_error: None,
            repaired: false,
            repaired_saved_path: None,
            recovered: false,
            recovery_available: false,
            recovery_load: false,
            recover_demo,
            notice: None,
        };

        if let Some(path) = initial {
            if shot_mode && !live_shot && !recover_demo {
                app.load_blocking(&cc.egui_ctx, &path);
            } else {
                app.begin_load(&cc.egui_ctx, path);
            }
        }
        if repair_open && !has_initial {
            match load_bytes_data(SAMPLE_VASE, "vase_broken.stl") {
                Ok(data) => {
                    app.apply_loaded(&cc.egui_ctx, data, 0.0);
                    app.open_repair(&cc.egui_ctx);
                }
                Err(error) => app.fail(error),
            }
        } else if (theater_demo || orientation_demo) && !has_initial {
            match load_bytes_data(SAMPLE_VASE, "vase_broken.stl") {
                Ok(data) => app.apply_loaded(&cc.egui_ctx, data, 0.0),
                Err(error) => app.fail(error),
            }
        }
        app
    }

    fn begin_load(&mut self, ctx: &egui::Context, path: PathBuf) {
        self.reset_stream();
        self.theater = None;
        self.camera_flight = None;
        let display = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "model".to_string());
        self.display_name = display.clone();
        self.file_path = Some(path.clone());
        self.last_dir = path.parent().map(Path::to_path_buf);
        self.phase = Phase::Loading {
            name: display.clone(),
            started: Instant::now(),
        };
        self.reset_repair();
        self.active_section = 0;
        self.scroll_to = None;
        self.scene = None;
        self.reports.clear();
        self.summary = MeshReport::default();
        self.markers.clear();
        self.gpu = None;
        self.visible.clear();
        self.selection = None;
        self.parse_ms = 0.0;
        self.upload_ms = 0.0;
        self.filters = [true; 4];
        self.recovered = false;
        self.recovery_available = false;
        self.recovery_load = false;

        self.rx = None;
        if let Some(total) = streaming::progressive_triangle_count(&path) {
            let (tx, rx) = mpsc::channel();
            self.stream = Some(LiveStream {
                started: Instant::now(),
                total,
                scan: ScanProgress {
                    total,
                    ..ScanProgress::default()
                },
                bounds: Bounds::empty(),
                positions: Vec::with_capacity(total.saturating_mul(3)),
                framed: false,
                follow: true,
                verifying: false,
            });
            self.stream_rx = Some(rx);
            streaming::spawn_stream(path, display, tx, ctx.clone());
        } else {
            let (tx, rx) = mpsc::channel();
            let ctx2 = ctx.clone();
            std::thread::spawn(move || {
                let result = load_path_data(&path, display);
                let _ = tx.send(result);
                ctx2.request_repaint();
            });
            self.rx = Some(rx);
        }
        ctx.request_repaint();
    }

    fn start_recovery(&mut self, ctx: &egui::Context) {
        if self.recovery_load {
            return;
        }
        let Some(path) = self.file_path.clone() else {
            return;
        };
        let display = self.display_name.clone();
        self.recovery_load = true;
        self.recovery_available = false;
        self.recovered = false;
        self.phase = Phase::Loading {
            name: display.clone(),
            started: Instant::now(),
        };
        self.reset_stream();
        self.theater = None;
        self.orientation = None;
        self.orientation_pose = None;
        self.orientation_job = None;
        self.orientation_error = None;
        self.camera_flight = None;
        self.reset_repair();
        self.active_section = 0;
        self.scroll_to = None;
        self.scene = None;
        self.reports.clear();
        self.summary = MeshReport::default();
        self.markers.clear();
        self.gpu = None;
        self.visible.clear();
        self.selection = None;
        self.parse_ms = 0.0;
        self.upload_ms = 0.0;
        self.filters = [true; 4];
        let (tx, rx) = mpsc::channel();
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                load_path_data_recovering(&path, display)
            }))
            .unwrap_or_else(|_| Err("The recovery worker stopped unexpectedly.".to_string()));
            let _ = tx.send(result);
            ctx2.request_repaint();
        });
        self.rx = Some(rx);
        ctx.request_repaint();
    }

    fn load_sample(&mut self, ctx: &egui::Context) {
        self.reset_stream();
        self.display_name = "vase_broken.stl".to_string();
        self.file_path = None;
        self.phase = Phase::Loading {
            name: "vase_broken.stl".to_string(),
            started: Instant::now(),
        };
        self.reset_repair();
        self.active_section = 0;
        self.scroll_to = None;
        self.scene = None;
        self.reports.clear();
        self.summary = MeshReport::default();
        self.markers.clear();
        self.gpu = None;
        self.visible.clear();
        self.selection = None;
        self.parse_ms = 0.0;
        self.upload_ms = 0.0;
        self.filters = [true; 4];
        self.recovered = false;
        self.recovery_available = false;
        self.recovery_load = false;

        let bytes = SAMPLE_VASE.to_vec();
        let (tx, rx) = mpsc::channel();
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let result = load_bytes_data(&bytes, "vase_broken.stl");
            let _ = tx.send(result);
            ctx2.request_repaint();
        });
        self.rx = Some(rx);
        ctx.request_repaint();
    }

    fn load_blocking(&mut self, ctx: &egui::Context, path: &Path) {
        let display = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "model".to_string());
        self.display_name = display.clone();
        self.file_path = Some(path.to_path_buf());
        self.last_dir = path.parent().map(Path::to_path_buf);
        let started = Instant::now();
        match load_path_data(path, display) {
            Ok(data) => self.apply_loaded(ctx, data, started.elapsed().as_secs_f64() * 1000.0),
            Err(error) => self.fail(error),
        }
    }

    fn fail(&mut self, message: String) {
        self.reset_stream();
        self.theater = None;
        self.camera_flight = None;
        let is_3mf = self
            .file_path
            .as_ref()
            .and_then(|path| path.extension())
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("3mf"))
            || self.display_name.to_ascii_lowercase().ends_with(".3mf");
        self.recovery_available = !self.recovery_load
            && is_3mf
            && (message.contains("external object part")
                || message.contains("missing object")
                || message.contains("object graph"));
        self.recovery_load = false;
        self.recovered = false;
        self.phase = Phase::Failed {
            title: "Could not read this file".to_string(),
            message,
        };
        self.reset_repair();
        self.active_section = 0;
        self.scroll_to = None;
        self.scene = None;
        self.reports.clear();
        self.summary = MeshReport::default();
        self.markers.clear();
        self.gpu = None;
    }

    fn install_scene(
        &mut self,
        scene: Scene,
        reports: Vec<MeshReport>,
        summary: MeshReport,
        markers: Vec<(usize, DefectMarker)>,
        display_name: String,
        reset_camera: bool,
    ) {
        self.reset_stream();
        self.theater = None;
        self.orientation = None;
        self.orientation_pose = None;
        self.orientation_job = None;
        self.orientation_error = None;
        self.camera_flight = None;
        self.recovery_load = false;
        self.recovery_available = false;
        if reset_camera {
            self.camera = Camera::framed(&scene.bounds());
            self.active_view = Some(ViewPreset::Home);
        }
        self.visible = vec![true; scene.objects.len()];
        self.selection = None;
        self.markers = markers;
        self.summary = summary;
        self.reports = reports;
        self.scene = Some(scene);
        self.gpu = None;
        self.gpu_defects_dirty = false;
        self.display_name = display_name;
        self.active_section = 0;
        self.scroll_to = None;
    }

    fn apply_loaded(&mut self, ctx: &egui::Context, data: LoadedData, _wall_ms: f64) {
        let keep_camera = self.stream.as_ref().is_some_and(|stream| !stream.follow);
        let recovery = data.recovery.clone();
        let markers = markers_from_reports(&data.reports);
        self.parse_ms = data.parse_ms;
        self.upload_ms = 0.0;
        self.repaired = false;
        self.repaired_saved_path = None;
        self.repair = None;
        self.repair_job = None;
        self.repair_open = false;
        self.repair_error = None;
        self.install_scene(
            data.scene,
            data.reports,
            data.summary,
            markers,
            data.display_name,
            !keep_camera,
        );
        self.phase = Phase::Ready;
        self.recovered = recovery.is_some();
        if let Some(report) = recovery {
            let plates = self
                .scene
                .as_ref()
                .map(|scene| scene.plates.len())
                .unwrap_or(0);
            self.set_notice(
                ctx,
                format!(
                    "Recovered {} objects across {} plate{}; skipped {} broken part{}.",
                    theme::fmt_int(report.loaded_objects),
                    plates,
                    if plates == 1 { "" } else { "s" },
                    theme::fmt_int(report.skipped_objects + report.skipped_components),
                    if report.skipped_objects + report.skipped_components == 1 {
                        ""
                    } else {
                        "s"
                    }
                ),
            );
        }
        if self.theater_demo
            && self.theater.is_none()
            && let Some((object, marker)) = self.markers.first().copied()
        {
            self.enter_theater(object, marker);
        }
        if self.orientation_demo {
            self.sidebar_open = true;
            self.active_section = 5;
            self.scroll_to = Some(5);
            self.start_orientation(ctx);
        }
        ctx.send_viewport_cmd(ViewportCommand::Title(format!(
            "ModelView — {}",
            self.display_name
        )));
        ctx.request_repaint();
    }

    fn poll(&mut self, ctx: &egui::Context) {
        let mut stream_events = Vec::new();
        let mut stream_closed = false;
        if let Some(rx) = &self.stream_rx {
            loop {
                match rx.try_recv() {
                    Ok(event) => stream_events.push(event),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        stream_closed = true;
                        break;
                    }
                }
            }
        }
        let mut stream_finished = false;
        for event in stream_events {
            match event {
                StreamEvent::Chunk(chunk) => {
                    if let Some(stream) = &mut self.stream {
                        stream.positions.extend_from_slice(&chunk.positions);
                        let bounds = chunk.scan.bounds;
                        stream.scan = chunk.scan;
                        stream.bounds = bounds;
                        if !stream.framed && !stream.bounds.is_empty() {
                            self.camera = Camera::framed(&stream.bounds);
                            self.active_view = None;
                            stream.framed = true;
                        }
                    }
                    ctx.request_repaint();
                }
                StreamEvent::Verifying(scan) => {
                    if let Some(stream) = &mut self.stream {
                        stream.scan = scan;
                        stream.verifying = true;
                    }
                    ctx.request_repaint();
                }
                StreamEvent::Finished(model) => {
                    stream_finished = true;
                    self.stream_rx = None;
                    self.apply_loaded(
                        ctx,
                        LoadedData {
                            scene: model.scene,
                            reports: model.reports,
                            summary: model.summary,
                            parse_ms: model.parse_ms,
                            display_name: self.display_name.clone(),
                            recovery: None,
                        },
                        0.0,
                    );
                }
                StreamEvent::Failed(error) => {
                    stream_finished = true;
                    self.stream_rx = None;
                    self.fail(error);
                }
            }
        }
        if stream_closed && !stream_finished && self.stream.is_some() {
            self.stream_rx = None;
            self.fail("The streaming loader stopped unexpectedly.".to_string());
        }
        let mut received = None;
        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok(result) => received = Some(result),
                Err(TryRecvError::Disconnected) => {
                    received = Some(Err("The loader stopped unexpectedly.".to_string()));
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        if let Some(result) = received {
            self.rx = None;
            let wall = match &self.phase {
                Phase::Loading { started, .. } => started.elapsed().as_secs_f64() * 1000.0,
                _ => 0.0,
            };
            match result {
                Ok(data) => self.apply_loaded(ctx, data, wall),
                Err(error) => {
                    let was_recovery = self.recovery_load;
                    self.fail(error);
                    if !was_recovery && self.recover_demo && self.recovery_available {
                        self.start_recovery(ctx);
                    }
                }
            }
        }
        let mut repair_result = None;
        if let Some(rx) = &self.repair_job {
            match rx.try_recv() {
                Ok(result) => repair_result = Some(result),
                Err(TryRecvError::Disconnected) => {
                    repair_result =
                        Some(Err("The repair worker stopped unexpectedly.".to_string()));
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        if let Some(result) = repair_result {
            self.repair_job = None;
            match result {
                Ok(preview) => {
                    let display = self.display_name.clone();
                    let stats = preview.stats;
                    if let Some(session) = &mut self.repair {
                        session.preview_active = true;
                        session.stats = stats;
                    }
                    self.install_scene(
                        preview.scene,
                        preview.reports,
                        preview.summary,
                        preview.markers,
                        display,
                        false,
                    );
                    self.repair_error = None;
                }
                Err(error) => {
                    self.repair_error = Some(error);
                    if let Some(session) = self.repair.as_mut() {
                        session.preview_active = false;
                        session.stats.clear();
                    }
                    if let Some(session) = self.repair.as_ref() {
                        let scene = session.original.clone();
                        let reports = session.original_reports.clone();
                        let summary = session.original_summary.clone();
                        let markers = session.original_markers.clone();
                        let display = self.display_name.clone();
                        self.install_scene(scene, reports, summary, markers, display, false);
                    }
                }
            }
            ctx.request_repaint();
        }
        if matches!(self.phase, Phase::Loading { .. }) {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn open_dialog(&mut self, ctx: &egui::Context) {
        let mut dialog = rfd::FileDialog::new()
            .add_filter("3D models", &["stl", "3mf", "STL", "3MF"])
            .set_title("Open a model");
        if let Some(dir) = &self.last_dir {
            dialog = dialog.set_directory(dir);
        }
        if let Some(path) = dialog.pick_file() {
            self.begin_load(ctx, path);
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let (
            open,
            frame_key,
            grid_key,
            escape,
            space,
            help,
            command,
            inspector,
            up,
            down,
            enter,
            next_defect,
            pressed,
            settings_key,
            send_key,
            repair_key,
        ) = ctx.input(|i| {
            (
                i.key_pressed(Key::O) && i.modifiers.command,
                i.key_pressed(Key::F),
                i.key_pressed(Key::G),
                i.key_pressed(Key::Escape),
                i.key_pressed(Key::Space),
                i.key_pressed(Key::H),
                i.modifiers.command,
                i.key_pressed(Key::I),
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::Enter),
                i.key_pressed(Key::D),
                [
                    i.key_pressed(Key::Num1),
                    i.key_pressed(Key::Num2),
                    i.key_pressed(Key::Num3),
                    i.key_pressed(Key::Num4),
                    i.key_pressed(Key::Num5),
                    i.key_pressed(Key::Num6),
                    i.key_pressed(Key::Num7),
                ],
                i.modifiers.command && i.key_pressed(Key::Comma),
                i.modifiers.command && i.key_pressed(Key::Enter),
                i.key_pressed(Key::R),
            )
        });

        if open {
            self.open_dialog(ctx);
        }
        if help {
            self.help_open = !self.help_open;
            if self.help_open {
                self.settings_open = false;
            }
        }
        if settings_key {
            self.settings_open = !self.settings_open;
            if self.settings_open {
                self.help_open = false;
            }
        }
        let orientation_key = ctx.input(|i| i.key_pressed(Key::O));
        if orientation_key && self.phase.is_ready() {
            self.sidebar_open = true;
            self.active_section = 5;
            self.scroll_to = Some(5);
        }
        if inspector && !self.help_open {
            self.sidebar_open = !self.sidebar_open;
        }
        let no_focus = ctx.memory(|memory| memory.focused().is_none());
        if escape {
            if self.settings_open {
                self.settings_open = false;
            } else if self.help_open {
                self.help_open = false;
            } else if self.repair_open {
                self.revert_repair(ctx);
            } else if self.theater.is_some() {
                self.exit_theater();
            } else if self.orientation_active() {
                self.reset_orientation_pose(ctx);
            } else {
                self.selection = None;
            }
        }
        if self.phase.is_ready() {
            if frame_key {
                let bounds = self
                    .orientation_bounds()
                    .or_else(|| self.scene.as_ref().map(|scene| scene.bounds()));
                if let Some(bounds) = bounds {
                    self.theater = None;
                    self.camera_flight = None;
                    self.camera.fit(&bounds);
                    self.active_view = None;
                }
            }
            if grid_key {
                self.show_grid = !self.show_grid;
            }
            if no_focus
                && (up || down)
                && let Some(scene) = &self.scene
            {
                let count = scene.objects.len();
                if count > 0 {
                    self.theater = None;
                    self.camera_flight = None;
                    let delta = if up { -1 } else { 1 };
                    let base = self.selection.map(|i| i as isize).unwrap_or(-1);
                    let next = (base + delta).rem_euclid(count as isize) as usize;
                    self.selection = Some(next);
                }
            }
            if send_key {
                self.send_or_settings(ctx);
            }
            if no_focus && repair_key {
                self.open_repair(ctx);
            }
            if no_focus
                && enter
                && !send_key
                && let Some(index) = self.selection
                && let Some(scene) = &self.scene
                && let Some(object) = scene.objects.get(index)
            {
                self.theater = None;
                self.camera_flight = None;
                self.camera.fit(&object.mesh.bounds());
                self.active_view = None;
            }
            if next_defect {
                self.focus_next_defect();
            }
            if no_focus
                && space
                && let Some(index) = self.selection
                && let Some(visible) = self.visible.get_mut(index)
            {
                *visible = !*visible;
                self.theater = None;
                self.camera_flight = None;
                self.gpu_defects_dirty = true;
            }
            if command {
                let presets = [
                    ViewPreset::Home,
                    ViewPreset::Top,
                    ViewPreset::Bottom,
                    ViewPreset::Front,
                    ViewPreset::Back,
                    ViewPreset::Left,
                    ViewPreset::Right,
                ];
                for (index, preset) in presets.into_iter().enumerate() {
                    if pressed[index] {
                        self.theater = None;
                        self.camera_flight = None;
                        self.camera.preset(preset);
                        self.active_view = Some(preset);
                    }
                }
            }
        }
    }

    fn handle_drops(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|file| file.path().to_path_buf())
                .collect()
        });
        if let Some(path) = dropped.into_iter().next() {
            self.begin_load(ctx, path);
        }
    }

    fn rebuild_defects(&mut self) {
        let focus = self.focus_params();
        if let Some(gpu) = &mut self.gpu {
            gpu.rebuild_defects(&self.reports, &self.visible, &self.filters, focus);
        }
        self.gpu_defects_dirty = false;
    }

    fn step_stream(&mut self, ctx: &egui::Context) {
        let Some(stream) = &self.stream else {
            return;
        };
        let targets = [
            stream.scan.open_edges as f32,
            stream.scan.non_manifold_edges as f32,
            stream.scan.inconsistent_edges as f32,
            stream.scan.volume_mm3 as f32,
            stream.scan.shells as f32,
            stream.scan.degenerate_triangles as f32,
        ];
        let mut moving = false;
        for (display, target) in self.scan_display.iter_mut().zip(targets) {
            let delta = target - *display;
            if delta.abs() > 0.01 {
                *display += delta * 0.18;
                moving = true;
            } else {
                *display = target;
            }
        }
        if moving {
            ctx.request_repaint();
        }
    }

    fn current_orientation_pose(&self) -> (Quat, Vec3) {
        let Some(pose) = self.orientation_pose else {
            return (Quat::IDENTITY, Vec3::ZERO);
        };
        let t = (pose.started.elapsed().as_secs_f32() / 0.55).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - t).powi(4);
        (
            pose.from_rotation.slerp(pose.to_rotation, eased),
            pose.from_translation.lerp(pose.to_translation, eased),
        )
    }

    fn orientation_active(&self) -> bool {
        self.orientation_pose.is_some()
            || self
                .orientation
                .as_ref()
                .is_some_and(|orientation| orientation.selected.is_some())
    }

    fn orientation_matrix(&self) -> Mat4 {
        let (rotation, translation) = self.current_orientation_pose();
        Mat4::from_rotation_translation(rotation, translation)
    }

    fn orientation_bounds(&self) -> Option<Bounds> {
        let orientation = self.orientation.as_ref()?;
        let index = orientation.selected?;
        let candidate = orientation.report.candidates.get(index)?;
        Some(Bounds {
            min: Vec3::new(
                -candidate.size_mm[0] * 0.5,
                -candidate.size_mm[1] * 0.5,
                0.0,
            ),
            max: Vec3::new(
                candidate.size_mm[0] * 0.5,
                candidate.size_mm[1] * 0.5,
                candidate.size_mm[2],
            ),
        })
    }

    fn start_orientation(&mut self, ctx: &egui::Context) {
        if self.orientation_job.is_some() || !self.phase.is_ready() {
            return;
        }
        let Some(scene) = self.scene.as_ref() else {
            return;
        };
        let scene = scene.clone();
        let mut identity_camera = self.camera;
        identity_camera.fit(&scene.bounds());
        self.orientation = None;
        self.orientation_error = None;
        self.orientation_pose = Some(OrientationPose {
            from_rotation: self.current_orientation_pose().0,
            from_translation: self.current_orientation_pose().1,
            to_rotation: Quat::IDENTITY,
            to_translation: Vec3::ZERO,
            started: Instant::now(),
        });
        self.camera_flight = Some(CameraFlight {
            from: self.camera,
            to: identity_camera,
            started: Instant::now(),
            duration: 0.45,
        });
        let (tx, rx) = mpsc::channel();
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                advise_orientation(&scene, Bed::default())
            }))
            .map_err(|_| "The orientation worker stopped unexpectedly.".to_string());
            let _ = tx.send(result);
            ctx2.request_repaint();
        });
        self.orientation_job = Some(rx);
        ctx.request_repaint();
    }

    fn select_orientation(&mut self, index: usize, ctx: &egui::Context) {
        let Some(orientation) = self.orientation.as_ref() else {
            return;
        };
        let Some(candidate) = orientation.report.candidates.get(index).cloned() else {
            return;
        };
        let (from_rotation, from_translation) = self.current_orientation_pose();
        let to_rotation = Quat::from_array(candidate.rotation);
        let to_translation = Vec3::from_array(candidate.translation);
        self.orientation_pose = Some(OrientationPose {
            from_rotation,
            from_translation,
            to_rotation,
            to_translation,
            started: Instant::now(),
        });
        if let Some(orientation) = &mut self.orientation {
            orientation.selected = Some(index);
        }
        let mut target = self.camera;
        target.fit(&Bounds {
            min: Vec3::new(
                -candidate.size_mm[0] * 0.5,
                -candidate.size_mm[1] * 0.5,
                0.0,
            ),
            max: Vec3::new(
                candidate.size_mm[0] * 0.5,
                candidate.size_mm[1] * 0.5,
                candidate.size_mm[2],
            ),
        });
        self.camera_flight = Some(CameraFlight {
            from: self.camera,
            to: target,
            started: Instant::now(),
            duration: 0.55,
        });
        self.active_view = None;
        ctx.request_repaint();
    }

    fn reset_orientation_pose(&mut self, ctx: &egui::Context) {
        let (from_rotation, from_translation) = self.current_orientation_pose();
        if (from_rotation - Quat::IDENTITY).length() < 1e-5 && from_translation.length() < 1e-5 {
            self.orientation_pose = None;
            if let Some(orientation) = &mut self.orientation {
                orientation.selected = None;
            }
            return;
        }
        self.orientation_pose = Some(OrientationPose {
            from_rotation,
            from_translation,
            to_rotation: Quat::IDENTITY,
            to_translation: Vec3::ZERO,
            started: Instant::now(),
        });
        if let Some(orientation) = &mut self.orientation {
            orientation.selected = None;
        }
        if let Some(scene) = &self.scene {
            let mut target = self.camera;
            target.fit(&scene.bounds());
            self.camera_flight = Some(CameraFlight {
                from: self.camera,
                to: target,
                started: Instant::now(),
                duration: 0.55,
            });
        }
        self.active_view = None;
        ctx.request_repaint();
    }

    fn clear_orientation_preview(&mut self) {
        self.orientation_pose = None;
        if let Some(orientation) = &mut self.orientation {
            orientation.selected = None;
        }
    }

    fn step_orientation(&mut self, ctx: &egui::Context) {
        if let Some(pose) = self.orientation_pose {
            if pose.started.elapsed().as_secs_f32() >= 0.55 {
                if self
                    .orientation
                    .as_ref()
                    .is_none_or(|orientation| orientation.selected.is_none())
                {
                    self.orientation_pose = None;
                }
            } else {
                ctx.request_repaint();
            }
        }
        if self.orientation_job.is_some() {
            ctx.request_repaint();
        }
        if let Some(rx) = &self.orientation_job {
            match rx.try_recv() {
                Ok(result) => {
                    self.orientation_job = None;
                    match result {
                        Ok(report) => {
                            self.orientation = Some(OrientationState {
                                report,
                                selected: None,
                            });
                            self.orientation_error = None;
                            self.select_orientation(0, ctx);
                        }
                        Err(error) => self.orientation_error = Some(error),
                    }
                    ctx.request_repaint();
                }
                Err(TryRecvError::Disconnected) => {
                    self.orientation_job = None;
                    self.orientation_error =
                        Some("The orientation worker stopped unexpectedly.".to_string());
                }
                Err(TryRecvError::Empty) => {}
            }
        }
    }

    fn theater_strength(&self) -> f32 {
        let Some(theater) = &self.theater else {
            return 0.0;
        };
        if let Some(closing) = theater.closing {
            return (1.0 - closing.elapsed().as_secs_f32() / 0.22).clamp(0.0, 1.0);
        }
        let t = (theater.started.elapsed().as_secs_f32() / 0.30).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    fn focus_params(&self) -> Option<Focus> {
        let theater = self.theater.as_ref()?;
        let strength = self.theater_strength();
        if strength <= 0.001 {
            return None;
        }
        Some(Focus {
            kind: theater.marker.kind,
            center: theater.marker.position,
            radius: theater.focus_radius,
            color: theme::kind_color(theater.marker.kind),
            amount: strength,
        })
    }

    fn enter_theater(&mut self, object: usize, marker: DefectMarker) {
        self.clear_orientation_preview();
        let Some(scene) = &self.scene else {
            return;
        };
        let previous_camera = self
            .theater
            .map(|theater| theater.previous_camera)
            .unwrap_or(self.camera);
        let bounds = scene
            .objects
            .get(object)
            .map(|object| object.mesh.bounds())
            .filter(|bounds| !bounds.is_empty())
            .unwrap_or_else(|| scene.bounds());
        let mut to = Camera::focus(&bounds, Vec3::from_array(marker.position), &self.camera);
        let local_lines = self
            .reports
            .get(object)
            .map(|report| local_defect_lines(report, &marker, bounds.radius().max(1.0) * 0.22))
            .unwrap_or_default();
        let marker_point = Vec3::from_array(marker.position);
        let local_extent = local_lines
            .iter()
            .flat_map(|line| {
                [
                    Vec3::from_array(line.a).distance(marker_point),
                    Vec3::from_array(line.b).distance(marker_point),
                ]
            })
            .fold(0.0_f32, f32::max);
        to.distance = (bounds.radius().max(0.5) * 1.8).max(local_extent * 3.2);
        let focus_radius = (local_extent * 1.35).max(bounds.radius().max(0.5) * 0.08);
        self.camera_flight = Some(CameraFlight {
            from: self.camera,
            to,
            started: Instant::now(),
            duration: 0.62,
        });
        self.theater = Some(Theater {
            object,
            marker,
            focus_radius,
            previous_camera,
            started: Instant::now(),
            closing: None,
        });
        self.selection = Some(object);
        self.active_view = None;
        self.gpu_defects_dirty = true;
    }

    fn exit_theater(&mut self) {
        let Some(theater) = self.theater else {
            return;
        };
        if theater.closing.is_some() {
            return;
        }
        if let Some(theater) = &mut self.theater {
            theater.closing = Some(Instant::now());
        }
        self.camera_flight = Some(CameraFlight {
            from: self.camera,
            to: theater.previous_camera,
            started: Instant::now(),
            duration: 0.55,
        });
        self.active_view = None;
        self.gpu_defects_dirty = true;
    }

    fn step_theater(&mut self, ctx: &egui::Context) {
        if let Some(flight) = self.camera_flight {
            let t = (flight.started.elapsed().as_secs_f32() / flight.duration).clamp(0.0, 1.0);
            let eased = 1.0 - (1.0 - t).powi(4);
            self.camera = Camera::lerp(&flight.from, &flight.to, eased);
            if t >= 1.0 {
                self.camera_flight = None;
            } else {
                ctx.request_repaint();
            }
        }
        let mut finish = false;
        let mut animate = false;
        if let Some(theater) = &self.theater {
            if let Some(closing) = theater.closing {
                if closing.elapsed().as_secs_f32() >= 0.22 {
                    finish = true;
                } else {
                    animate = true;
                }
            } else if theater.started.elapsed().as_secs_f32() < 0.40 {
                animate = true;
            }
        }
        if finish {
            self.theater = None;
            self.gpu_defects_dirty = true;
        }
        if animate {
            ctx.request_repaint();
        }
    }

    fn focus_marker(&mut self, object: usize, marker: &DefectMarker) {
        self.enter_theater(object, *marker);
    }

    fn focus_kind(&mut self, kind: DefectKind) {
        let found = self
            .markers
            .iter()
            .find(|(object, marker)| {
                marker.kind == kind
                    && self.visible.get(*object).copied().unwrap_or(true)
                    && self.filters[crate::gpu::defect_slot(kind)]
            })
            .copied();
        if let Some((object, marker)) = found {
            self.focus_marker(object, &marker);
        }
    }

    fn focus_next_defect(&mut self) {
        let candidates: Vec<(usize, DefectMarker)> = self
            .markers
            .iter()
            .filter(|(object, marker)| {
                self.visible.get(*object).copied().unwrap_or(true)
                    && self.filters[crate::gpu::defect_slot(marker.kind)]
            })
            .copied()
            .collect();
        if candidates.is_empty() {
            return;
        }
        let index = self.defect_cursor % candidates.len();
        let (object, marker) = candidates[index];
        self.defect_cursor = index + 1;
        self.focus_marker(object, &marker);
    }

    fn check_rows(&self) -> Vec<CheckRow> {
        let s = &self.summary;
        let degenerate = s.degenerate_triangles + s.invalid_triangles;
        let inward = self
            .reports
            .iter()
            .any(|report| report.triangles > 0 && report.signed_volume_mm3 <= 0.0);
        vec![
            CheckRow {
                label: "Closed surface",
                detail: "Every edge joins exactly two faces",
                value: if s.open_edges == 0 {
                    "Closed".into()
                } else {
                    format!("{} open", theme::fmt_int(s.open_edges))
                },
                state: if s.open_edges == 0 {
                    theme::StateMark::Pass
                } else {
                    theme::StateMark::Fail
                },
                kind: Some(DefectKind::OpenEdge),
            },
            CheckRow {
                label: "Manifold joins",
                detail: "No edge shared by three or more faces, no split vertex fan",
                value: if s.non_manifold_edges + s.non_manifold_vertices == 0 {
                    "Clean".into()
                } else {
                    format!(
                        "{} joins",
                        theme::fmt_int(s.non_manifold_edges + s.non_manifold_vertices)
                    )
                },
                state: if s.non_manifold_edges + s.non_manifold_vertices == 0 {
                    theme::StateMark::Pass
                } else {
                    theme::StateMark::Fail
                },
                kind: Some(DefectKind::NonManifold),
            },
            CheckRow {
                label: "Winding",
                detail: "All faces agree on which way is out",
                value: if s.inconsistent_edges == 0 {
                    "Agrees".into()
                } else {
                    format!("{} flipped", theme::fmt_int(s.inconsistent_edges))
                },
                state: if s.inconsistent_edges == 0 {
                    theme::StateMark::Pass
                } else {
                    theme::StateMark::Fail
                },
                kind: Some(DefectKind::InconsistentWinding),
            },
            CheckRow {
                label: "Volume",
                detail: "Signed volume with outward normals",
                value: if inward {
                    "Inward".into()
                } else {
                    theme::fmt_volume(s.volume_mm3)
                },
                state: if s.triangles == 0 {
                    theme::StateMark::Info
                } else if inward {
                    theme::StateMark::Warn
                } else {
                    theme::StateMark::Pass
                },
                kind: None,
            },
            CheckRow {
                label: "Shells",
                detail: "Separate closed surface groups",
                value: theme::fmt_int(s.shells),
                state: theme::StateMark::Info,
                kind: None,
            },
            CheckRow {
                label: "Degenerate faces",
                detail: "Zero-area triangles",
                value: if degenerate == 0 {
                    "None".into()
                } else {
                    theme::fmt_int(degenerate)
                },
                state: if degenerate == 0 {
                    theme::StateMark::Pass
                } else {
                    theme::StateMark::Warn
                },
                kind: if degenerate > 0 {
                    Some(DefectKind::DegenerateFace)
                } else {
                    None
                },
            },
        ]
    }

    fn verdict(&self) -> Verdict {
        if self.reports.is_empty() {
            return Verdict::Failed;
        }
        let mut worst = Verdict::Watertight;
        for report in &self.reports {
            match report.verdict() {
                Verdict::Failed => return Verdict::Failed,
                Verdict::Warning => worst = Verdict::Warning,
                Verdict::Watertight => {}
            }
        }
        worst
    }

    fn status_headline(&self) -> String {
        match self.verdict() {
            Verdict::Watertight => self.summary.headline(),
            Verdict::Warning => self
                .reports
                .iter()
                .find(|report| report.verdict() == Verdict::Warning)
                .map(|report| report.headline())
                .unwrap_or_else(|| self.summary.headline()),
            Verdict::Failed => self
                .reports
                .iter()
                .find(|report| report.verdict() == Verdict::Failed)
                .map(|report| report.headline())
                .unwrap_or_else(|| self.summary.headline()),
        }
    }

    fn section_labels(&self) -> [&'static str; 6] {
        match &self.phase {
            Phase::Failed { .. } => ["VERDICT", "CHECKS", "", "", "DETAIL", ""],
            Phase::Loading { .. } => ["VERDICT", "CHECKS", "", "", "FILE", ""],
            Phase::Ready => {
                let multi = self
                    .scene
                    .as_ref()
                    .map(|scene| scene.objects.len() > 1 || !scene.plates.is_empty())
                    .unwrap_or(false);
                if multi {
                    ["VERDICT", "CHECKS", "OBJECTS", "MODEL", "FILE", "ORIENT"]
                } else {
                    ["VERDICT", "CHECKS", "MODEL", "OBJECTS", "FILE", "ORIENT"]
                }
            }
            Phase::Empty => ["VERDICT", "CHECKS", "MODEL", "OBJECTS", "FILE", ""],
        }
    }

    fn reset_stream(&mut self) {
        self.stream = None;
        self.stream_rx = None;
        self.live_gpu = None;
        self.live_uploaded = 0;
        self.scan_display = [0.0; 6];
        self.orientation = None;
        self.orientation_pose = None;
        self.orientation_job = None;
        self.orientation_error = None;
    }

    fn reset_repair(&mut self) {
        self.repair = None;
        self.repair_job = None;
        self.repair_open = false;
        self.repair_error = None;
        self.repaired = false;
        self.repaired_saved_path = None;
    }

    fn set_notice(&mut self, ctx: &egui::Context, text: String) {
        self.notice = Some((text, Instant::now()));
        ctx.request_repaint();
    }

    fn send_or_settings(&mut self, ctx: &egui::Context) {
        if self.repair_job.is_some() {
            self.set_notice(ctx, "The repair preview is still running.".to_string());
            return;
        }
        if self.repair_open
            && !self
                .repair
                .as_ref()
                .is_some_and(|session| session.preview_active)
        {
            self.set_notice(ctx, "The repair preview is not ready.".to_string());
            return;
        }
        if self.settings.slicer.is_none() {
            self.settings_open = true;
            self.help_open = false;
        } else {
            self.send_to_slicer(ctx);
        }
    }

    fn send_to_slicer(&mut self, ctx: &egui::Context) {
        let Some(choice) = self.settings.slicer.clone() else {
            self.settings_open = true;
            self.help_open = false;
            return;
        };
        let repaired = self
            .repair
            .as_ref()
            .map(|session| session.preview_active)
            .unwrap_or(self.repaired)
            || self.recovered;
        let saved = if let Some(session) = &self.repair {
            if session.preview_active && session.saved_revision == Some(session.revision) {
                session.saved_path.clone()
            } else {
                None
            }
        } else if self.repaired || self.recovered {
            self.repaired_saved_path.clone()
        } else {
            None
        };

        let result = match self.scene.as_ref() {
            Some(scene) => {
                let input = LaunchInput {
                    scene,
                    source_path: self.file_path.as_deref(),
                    repaired,
                    saved_repaired_path: saved.as_deref(),
                };
                working_file::prepare_launch_file(input).and_then(|file| {
                    slicer::launch(&choice, &file.path)?;
                    Ok(file)
                })
            }
            None => Err(anyhow::anyhow!("No model is loaded.")),
        };

        match result {
            Ok(file) => {
                let text = if file.temporary {
                    if file.repaired && self.recovered {
                        format!("Opened a temporary recovered copy in {}", choice.name)
                    } else if file.repaired {
                        format!("Opened a temporary repaired copy in {}", choice.name)
                    } else {
                        format!("Opened a temporary copy in {}", choice.name)
                    }
                } else {
                    format!(
                        "Opened {} in {}",
                        file.path
                            .file_name()
                            .map(|name| name.to_string_lossy().to_string())
                            .unwrap_or_else(|| file.path.display().to_string()),
                        choice.name
                    )
                };
                self.repair_error = None;
                self.set_notice(ctx, text);
            }
            Err(error) => {
                let text = format!("Could not open {}: {error}", choice.name);
                if self.repair_open {
                    self.repair_error = Some(text);
                } else {
                    self.set_notice(ctx, text);
                }
            }
        }
    }

    fn save_repaired_copy(&mut self, ctx: &egui::Context) {
        let Some(scene) = self.scene.as_ref() else {
            return;
        };
        let extension = match scene.format {
            Format::Stl => "stl",
            Format::ThreeMf => "3mf",
        };
        let stem = self
            .file_path
            .as_ref()
            .and_then(|path| path.file_stem())
            .map(|stem| stem.to_string_lossy().to_string())
            .filter(|stem| !stem.is_empty())
            .unwrap_or_else(|| {
                self.display_name
                    .trim_end_matches(".stl")
                    .trim_end_matches(".3mf")
                    .to_string()
            });
        let default_name = format!("{stem}.repaired.{extension}");
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Repaired model", &[extension])
            .set_file_name(&default_name)
            .set_title("Save repaired copy");
        if let Some(dir) = &self.last_dir {
            dialog = dialog.set_directory(dir);
        }
        let Some(path) = dialog.save_file() else {
            return;
        };
        let same_as_source = self.file_path.as_deref() == Some(path.as_path())
            || (self.file_path.is_some()
                && self
                    .file_path
                    .as_ref()
                    .and_then(|source| source.canonicalize().ok())
                    == path.canonicalize().ok());
        if same_as_source {
            self.repair_error =
                Some("The source file is never overwritten. Choose a different name.".to_string());
            ctx.request_repaint();
            return;
        }

        let result: anyhow::Result<()> = (|| {
            let Some(scene) = self.scene.as_ref() else {
                return Ok(());
            };
            match scene.format {
                Format::Stl => {
                    let Some(object) = scene.objects.first() else {
                        return Ok(());
                    };
                    write_stl(&path, &object.mesh)?;
                }
                Format::ThreeMf => {
                    let settings = self
                        .file_path
                        .as_deref()
                        .filter(|source| {
                            source.exists()
                                && source
                                    .extension()
                                    .map(|ext| ext.eq_ignore_ascii_case("3mf"))
                                    .unwrap_or(false)
                        })
                        .and_then(|source| read_project_settings(source).ok())
                        .flatten();
                    write_3mf(&path, scene, settings.as_deref())?;
                }
            }
            Ok(())
        })();

        match result {
            Ok(()) => {
                if let Some(session) = &mut self.repair {
                    session.saved_path = Some(path.clone());
                    session.saved_revision = Some(session.revision);
                }
                self.repaired_saved_path = Some(path.clone());
                self.repair_error = None;
                self.set_notice(ctx, format!("Saved repaired copy to {}", path.display()));
            }
            Err(error) => {
                let text = format!("Could not save repaired copy: {error}");
                if self.repair_open {
                    self.repair_error = Some(text);
                } else {
                    self.set_notice(ctx, text);
                }
            }
        }
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let candidates = self.slicer_candidates.clone();
        let selected = self.settings.slicer.clone();
        let storage_available = self.storage_available;
        let mut chosen: Option<SlicerCandidate> = None;
        let mut open = true;
        egui::Window::new("Settings")
            .id(Id::unique("settings_window"))
            .order(egui::Order::Foreground)
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .fade_in(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .frame(sheet_frame())
            .show(ctx, |ui| {
                ui.set_width(540.0);
                theme::label(ui, "SETTINGS / SLICER", 9.0, theme::narrow_semi(), theme::BONE, 2.4);
                ui.add_space(6.0);
                ui.label(
                    RichText::new("ModelView opens the current model in the slicer you choose. Files stay on this machine.")
                        .font(FontId::new(14.5, egui::FontFamily::Proportional))
                        .color(theme::DIM),
                );
                ui.add_space(16.0);
                if candidates.is_empty() {
                    theme::label(
                        ui,
                        "No slicer detected. Choose an app or executable below.",
                        12.0,
                        theme::narrow(),
                        theme::DIM,
                        0.4,
                    );
                    ui.add_space(8.0);
                }
                for candidate in &candidates {
                    if slicer_row(ui, candidate, selected.as_ref()) {
                        chosen = Some(candidate.clone());
                    }
                }
                if let Some(current) = &selected {
                    let represented = candidates
                        .iter()
                        .any(|c| c.kind == current.kind && c.path == current.path);
                    if current.path.exists() {
                        if !represented {
                            ui.add_space(8.0);
                            let candidate = SlicerCandidate {
                                name: current.name.clone(),
                                kind: current.kind,
                                path: current.path.clone(),
                            };
                            if slicer_row(ui, &candidate, selected.as_ref()) {
                                chosen = Some(candidate);
                            }
                        }
                    } else if !represented {
                        ui.add_space(8.0);
                        theme::label(
                            ui,
                            format!("CURRENT / {} — MISSING", current.name.to_uppercase()),
                            10.5,
                            theme::narrow_semi(),
                            theme::RED_TEXT,
                            1.3,
                        );
                        theme::label(
                            ui,
                            truncate(&current.path.display().to_string(), 64),
                            11.5,
                            theme::narrow(),
                            theme::DIM,
                            0.2,
                        );
                    }
                }
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    if sheet_button(ui, "BROWSE…", 150.0, false)
                        && let Some(path) = rfd::FileDialog::new()
                            .set_title("Choose a slicer app or executable")
                            .pick_file()
                    {
                        let name = path
                            .file_stem()
                            .map(|stem| stem.to_string_lossy().to_string())
                            .unwrap_or_else(|| "Slicer".to_string());
                        chosen = Some(SlicerCandidate {
                            name,
                            kind: SlicerKind::Custom,
                            path,
                        });
                    }
                    if sheet_button(ui, "RESCAN", 120.0, false) {
                        self.slicer_candidates = slicer::detect_slicers();
                    }
                });
                ui.add_space(14.0);
                let selected_text = chosen
                    .as_ref()
                    .map(|c| format!("SELECTED / {}", c.name))
                    .or_else(|| selected.as_ref().map(|c| format!("SELECTED / {}", c.name)))
                    .unwrap_or_else(|| "NO SLICER SELECTED".to_string());
                theme::label(ui, selected_text, 10.5, theme::narrow_semi(), theme::BONE, 1.6);
                ui.add_space(4.0);
                theme::label(
                    ui,
                    if storage_available {
                        "Changes save when you close ModelView."
                    } else {
                        "Storage is unavailable; this choice lasts for this session."
                    },
                    10.5,
                    theme::narrow(),
                    theme::DIM,
                    0.5,
                );
                ui.add_space(16.0);
                if sheet_button(ui, "CLOSE", 110.0, true) {
                    open = false;
                }
            });
        if let Some(candidate) = chosen {
            self.settings.slicer = Some(SlicerChoice {
                name: candidate.name,
                kind: candidate.kind,
                path: candidate.path,
            });
        }
        if !open {
            self.settings_open = false;
        }
    }

    fn open_repair(&mut self, ctx: &egui::Context) {
        if self.repair_open || self.scene.is_none() || self.verdict() == Verdict::Watertight {
            return;
        }
        self.theater = None;
        self.orientation = None;
        self.orientation_pose = None;
        self.orientation_job = None;
        self.orientation_error = None;
        self.camera_flight = None;
        let Some(scene) = self.scene.clone() else {
            return;
        };
        let session = RepairSession {
            original: scene,
            original_reports: self.reports.clone(),
            original_summary: self.summary.clone(),
            original_markers: self.markers.clone(),
            options: RepairOptions::default(),
            deep_repair: false,
            revision: 1,
            saved_path: None,
            saved_revision: None,
            preview_active: false,
            stats: Vec::new(),
            original_repaired: self.repaired,
            original_saved_path: self.repaired_saved_path.clone(),
        };
        self.repair_job = Some(repair_flow::start_repair(
            session.original.clone(),
            session.options,
            session.deep_repair,
            ctx.clone(),
        ));
        self.repair = Some(session);
        self.repair_open = true;
        self.repair_error = None;
        self.settings_open = false;
        self.help_open = false;
        ctx.request_repaint();
    }

    fn revert_repair(&mut self, ctx: &egui::Context) {
        let Some(session) = self.repair.take() else {
            self.repair_open = false;
            return;
        };
        self.install_scene(
            session.original,
            session.original_reports,
            session.original_summary,
            session.original_markers,
            self.display_name.clone(),
            false,
        );
        self.repair_job = None;
        self.repair_open = false;
        self.repair_error = None;
        self.repaired = session.original_repaired;
        self.repaired_saved_path = session.original_saved_path;
        ctx.request_repaint();
    }

    fn sheet_area_bounds(&self, ctx: &egui::Context) -> Option<Rect> {
        let mut ids = Vec::new();
        if self.repair_open {
            ids.push(Id::unique("repair_window"));
        }
        if self.settings_open {
            ids.push(Id::unique("settings_window"));
        }
        if self.help_open {
            ids.push(Id::unique("controls_window"));
        }
        if ids.is_empty() {
            return None;
        }
        ctx.memory(|memory| {
            ids.into_iter()
                .filter_map(|id| memory.area_rect(id))
                .reduce(Rect::union)
        })
    }

    fn sheet_clips_topbar(&self, ctx: &egui::Context) -> bool {
        self.sheet_area_bounds(ctx)
            .is_some_and(|bounds| bounds.min.y < 66.0)
    }

    fn sheet_clips_status(&self, ctx: &egui::Context) -> bool {
        let screen = ctx.content_rect();
        self.sheet_area_bounds(ctx)
            .is_some_and(|bounds| bounds.max.y > screen.max.y - 24.0)
    }

    fn sheet_clips_inspector(&self, ctx: &egui::Context) -> bool {
        let screen = ctx.content_rect();
        self.sheet_area_bounds(ctx).is_some_and(|bounds| {
            bounds.max.x > screen.max.x - 392.0
                && bounds.min.y < screen.max.y - 38.0
                && bounds.max.y > 66.0
        })
    }

    fn sheet_backdrop(&self, ctx: &egui::Context) {
        let overlay = self.settings_open || self.help_open;
        if !overlay && !self.repair_open {
            return;
        }
        let Some(viewport) = self.viewport_rect else {
            return;
        };
        let screen = ctx.content_rect();
        let painter = ctx.layer_painter(egui::LayerId::background());
        if overlay {
            painter.rect_filled(
                screen,
                egui::CornerRadius::ZERO,
                Color32::from_black_alpha(200),
            );
            return;
        }
        let color = Color32::from_black_alpha(200);
        painter.rect_filled(
            Rect::from_min_max(screen.min, Pos2::new(screen.max.x, viewport.min.y)),
            egui::CornerRadius::ZERO,
            color,
        );
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(screen.min.x, viewport.min.y),
                Pos2::new(viewport.min.x, viewport.max.y),
            ),
            egui::CornerRadius::ZERO,
            color,
        );
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(viewport.max.x, viewport.min.y),
                Pos2::new(screen.max.x, viewport.max.y),
            ),
            egui::CornerRadius::ZERO,
            color,
        );
    }

    fn repair_window(&mut self, ctx: &egui::Context) {
        if !self.repair_open {
            return;
        }
        let Some(mut session) = self.repair.take() else {
            self.repair_open = false;
            return;
        };
        let job_running = self.repair_job.is_some();
        let error = self.repair_error.clone();
        let before = session.original_summary.clone();
        let after = if session.preview_active {
            Some(self.summary.clone())
        } else {
            None
        };
        let after_color = if after.is_some()
            && self
                .reports
                .iter()
                .all(|report| report.verdict() == Verdict::Watertight)
        {
            theme::BONE
        } else {
            theme::RED_TEXT
        };
        let saved_path = session.saved_path.clone();
        let notice = self
            .notice
            .as_ref()
            .filter(|(text, at)| {
                at.elapsed() < Duration::from_secs(8) && text.starts_with("Opened")
            })
            .map(|(text, _)| format!("NOTICE / {}", truncate(text, 60)));
        let slicer = self.settings.slicer.clone();
        let send_label = match &slicer {
            Some(choice) => format!("SEND TO {}", truncate(&choice.name.to_uppercase(), 16)),
            None => "SET UP SLICER".to_string(),
        };
        let stats = session.stats.clone();
        let weld_locked = session.original.format == Format::Stl;
        let mut options = session.options;
        if weld_locked {
            options.weld = true;
        }
        let mut deep_repair = session.deep_repair;
        let mut changed = false;
        let mut action: Option<RepairAction> = None;
        let mut open = true;
        let welded: usize = stats.iter().map(|s| s.welded_vertices).sum();
        let degenerate: usize = stats.iter().map(|s| s.degenerate_removed).sum();
        let duplicates: usize = stats.iter().map(|s| s.duplicates_removed).sum();
        let flipped: usize = stats.iter().map(|s| s.faces_flipped).sum();
        let holes: usize = stats.iter().map(|s| s.holes_filled).sum();
        let patches: usize = stats.iter().map(|s| s.patch_faces).sum();
        let healed_edges: usize = stats.iter().map(|s| s.non_manifold_edges_healed).sum();
        let healed_vertices: usize = stats.iter().map(|s| s.non_manifold_vertices_healed).sum();
        let rolled_back: usize = stats.iter().map(|s| s.passes_rolled_back).sum();
        let deep_remeshed = stats.iter().any(|s| s.deep_remeshed);
        let voxel_size = stats
            .iter()
            .find(|s| s.deep_remeshed)
            .map(|s| s.voxel_size_mm)
            .unwrap_or(0.0);
        let no_change = !deep_remeshed
            && welded + degenerate + duplicates + flipped + holes + healed_edges + healed_vertices
                == 0;
        let compact = ctx.content_rect().height() < 760.0;

        egui::Window::new("Repair")
            .id(Id::unique("repair_window"))
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .fade_in(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .frame(sheet_frame())
            .show(ctx, |ui| {
                ui.set_width(540.0);
                theme::label(
                    ui,
                    format!("REPAIR / {}", truncate(&self.display_name, 46)),
                    9.0,
                    theme::narrow_semi(),
                    theme::BONE,
                    2.2,
                );
                ui.add_space(8.0);
                if job_running {
                    let progress = (ui.ctx().input(|i| i.time) * 0.55).fract() as f32;
                    let (bar, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 2.0), Sense::hover());
                    ui.painter().hline(bar.x_range(), bar.center().y, Stroke::new(1.0, theme::LINE));
                    let x = bar.left() + (bar.width() - 90.0) * progress;
                    ui.painter().hline(x..=x + 90.0, bar.center().y, Stroke::new(2.0, theme::BLUE));
                    ui.add_space(4.0);
                    theme::label(ui, "REPAIRING / THE PREVIEW WILL LAND HERE", 10.5, theme::narrow_semi(), theme::BLUE, 1.4);
                } else if let Some(after) = &after {
                    repair_metrics(ui, "BEFORE", &before, theme::DIM);
                    repair_metrics(ui, "AFTER", after, after_color);
                    if no_change {
                        ui.add_space(4.0);
                        theme::label(
                            ui,
                            "NO SAFE REPAIR APPLIED",
                            10.5,
                            theme::narrow_semi(),
                            theme::BONE,
                            1.4,
                        );
                    }
                    ui.add_space(4.0);
                    if deep_remeshed {
                        theme::label(
                            ui,
                            format!(
                                "DEEP REMESH / ~{:.2} MM / SDF REBUILD",
                                voxel_size
                            ),
                            10.0,
                            theme::narrow(),
                            theme::BONE,
                            0.8,
                        );
                    } else {
                        theme::label(
                            ui,
                            format!(
                                "WELDED {} / DEGENERATE {} / DUPLICATES {} / FLIPPED {} / HOLES {} / PATCHES {}",
                                theme::fmt_int(welded),
                                theme::fmt_int(degenerate),
                                theme::fmt_int(duplicates),
                                theme::fmt_int(flipped),
                                theme::fmt_int(holes),
                                theme::fmt_int(patches)
                            ),
                            10.0,
                            theme::narrow(),
                            theme::DIM,
                            0.8,
                        );
                        if healed_edges + healed_vertices + rolled_back > 0 {
                            theme::label(
                                ui,
                                format!(
                                    "JOINS {} EDGES / {} VERTICES{}",
                                    theme::fmt_int(healed_edges),
                                    theme::fmt_int(healed_vertices),
                                    if rolled_back > 0 {
                                        format!(" / {} PASSES DECLINED", theme::fmt_int(rolled_back))
                                    } else {
                                        String::new()
                                    }
                                ),
                                10.0,
                                theme::narrow(),
                                theme::DIM,
                                0.8,
                            );
                        }
                    }
                    if let Some(saved) = &saved_path {
                        ui.add_space(4.0);
                        theme::label(
                            ui,
                            format!("SAVED / {}", truncate(&saved.display().to_string(), 62)),
                            10.0,
                            theme::narrow(),
                            theme::DIM,
                            0.4,
                        );
                    }
                } else {
                    repair_metrics(ui, "BEFORE", &before, theme::DIM);
                }
                if let Some(error) = &error {
                    ui.add_space(8.0);
                    theme::label(ui, "REPAIR ERROR", 10.0, theme::narrow_semi(), theme::RED_TEXT, 1.8);
                    ui.label(RichText::new(error).font(FontId::new(13.5, egui::FontFamily::Proportional)).color(theme::RED_TEXT));
                }
                if let Some(notice) = &notice {
                    ui.add_space(6.0);
                    theme::label(ui, notice, 9.5, theme::narrow(), theme::BONE, 0.5);
                }
                ui.add_space(if compact { 8.0 } else { 14.0 });
                let disabled = job_running;
                let safe_disabled = disabled || deep_repair;
                if repair_option_row(
                    ui,
                    if weld_locked {
                        "Weld coincident vertices (STL)"
                    } else {
                        "Weld coincident vertices"
                    },
                    &mut options.weld,
                    safe_disabled,
                    weld_locked,
                    theme::fmt_int(welded),
                ) {
                    changed = true;
                }
                if repair_option_row(ui, "Remove degenerate faces", &mut options.remove_degenerate, safe_disabled, false, theme::fmt_int(degenerate)) { changed = true; }
                if repair_option_row(ui, "Remove duplicate faces", &mut options.remove_duplicate_faces, safe_disabled, false, theme::fmt_int(duplicates)) { changed = true; }
                if repair_option_row(
                    ui,
                    "Split non-manifold joins",
                    &mut options.heal_non_manifold,
                    safe_disabled,
                    false,
                    format!(
                        "{} EDGES / {} VERTICES",
                        theme::fmt_int(healed_edges),
                        theme::fmt_int(healed_vertices)
                    ),
                ) {
                    changed = true;
                }
                if repair_option_row(ui, "Fix winding", &mut options.fix_winding, safe_disabled, false, theme::fmt_int(flipped)) { changed = true; }
                let loop_word = if holes == 1 { "LOOP" } else { "LOOPS" };
                if repair_option_row(ui, "Fill boundary holes", &mut options.fill_holes, safe_disabled, false, format!("{holes} {loop_word} / {patches} PATCHES")) { changed = true; }
                let deep_detail = if voxel_size > 0.0 {
                    format!("~{:.2} MM", voxel_size)
                } else {
                    "AUTO".to_string()
                };
                if repair_option_row(
                    ui,
                    "Deep remesh (rebuild geometry)",
                    &mut deep_repair,
                    disabled,
                    false,
                    deep_detail,
                ) {
                    changed = true;
                }
                ui.add_space(if compact { 6.0 } else { 10.0 });
                if deep_repair {
                    theme::label(
                        ui,
                        "Deep remesh rebuilds the whole object on a signed distance grid. Sharp detail and thin features may change; only a watertight result is kept. The source is never overwritten.",
                        10.0,
                        theme::narrow(),
                        theme::DIM,
                        0.4,
                    );
                } else {
                    theme::label(
                        ui,
                        "Boundary holes are patched with a simple fan. Non-manifold joins are split only when the AFTER report improves; remaining joins stay unresolved. Self-intersections are not checked.",
                        10.0,
                        theme::narrow(),
                        theme::DIM,
                        0.4,
                    );
                }
                ui.add_space(if compact { 10.0 } else { 16.0 });
                let keep_enabled = !job_running && after.is_some() && !no_change;
                let io_enabled = !job_running && after.is_some();
                ui.horizontal(|ui| {
                    if sheet_button_enabled(ui, "KEEP REPAIRED", 240.0, true, keep_enabled) {
                        action = Some(RepairAction::Keep);
                    }
                    if sheet_button(ui, "REVERT", 120.0, false) {
                        action = Some(RepairAction::Revert);
                    }
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if sheet_button_enabled(
                        ui,
                        "SAVE REPAIRED COPY…",
                        240.0,
                        false,
                        io_enabled,
                    ) {
                        action = Some(RepairAction::Save);
                    }
                    if sheet_button_enabled(ui, &send_label, 200.0, slicer.is_some(), io_enabled)
                    {
                        action = Some(RepairAction::Send);
                    }
                });
                ui.add_space(8.0);
                if sheet_button(ui, "CLOSE", 110.0, false) {
                    open = false;
                }
            });

        if !open && action.is_none() {
            action = Some(RepairAction::Revert);
        }
        if changed && !job_running {
            session.options = options;
            session.deep_repair = deep_repair;
            session.revision += 1;
            session.saved_path = None;
            session.saved_revision = None;
            self.repair_job = Some(repair_flow::start_repair(
                session.original.clone(),
                options,
                deep_repair,
                ctx.clone(),
            ));
            self.repair_error = None;
        }

        match action {
            Some(RepairAction::Keep) => {
                self.repaired = true;
                self.repaired_saved_path = session.saved_path.clone();
                self.repair_open = false;
                self.repair_job = None;
                self.repair_error = None;
            }
            Some(RepairAction::Revert) => {
                self.install_scene(
                    session.original,
                    session.original_reports,
                    session.original_summary,
                    session.original_markers,
                    self.display_name.clone(),
                    false,
                );
                self.repair_open = false;
                self.repair_job = None;
                self.repair_error = None;
                self.repaired = session.original_repaired;
                self.repaired_saved_path = session.original_saved_path;
            }
            Some(RepairAction::Save) => {
                self.repair = Some(session);
                self.save_repaired_copy(ctx);
            }
            Some(RepairAction::Send) => {
                self.repair = Some(session);
                self.send_or_settings(ctx);
            }
            None => {
                self.repair = Some(session);
            }
        }
        if self.repair_job.is_some() {
            ctx.request_repaint();
        }
    }

    fn verdict_actions(&mut self, ui: &mut Ui, verdict: Verdict) {
        if self.scene.is_none() {
            return;
        }
        ui.add_space(10.0);
        let slicer = self.settings.slicer.clone();
        let send_label = match &slicer {
            Some(choice) => format!("SEND TO {}", truncate(&choice.name.to_uppercase(), 20)),
            None => "SET UP SLICER".to_string(),
        };
        let needs_repair = verdict != Verdict::Watertight;
        let width = ui.available_width();
        ui.horizontal(|ui| {
            if needs_repair && sheet_button(ui, "REPAIR…", (width - 8.0) * 0.5, true) {
                self.open_repair(ui.ctx());
            }
            let send_width = if needs_repair {
                (width - 8.0) * 0.5
            } else {
                width
            };
            if sheet_button(ui, &send_label, send_width, !needs_repair) {
                self.send_or_settings(ui.ctx());
            }
        });
        ui.add_space(2.0);
    }

    fn top_bar(&mut self, ui: &mut Ui) {
        if self.sheet_clips_topbar(ui.ctx()) {
            return;
        }
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.add_space(20.0);
            let (mark, _) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
            let painter = ui.painter();
            painter.rect_stroke(
                mark,
                egui::CornerRadius::ZERO,
                Stroke::new(1.0, theme::LINE2),
                StrokeKind::Inside,
            );
            painter.vline(
                mark.center().x,
                mark.y_range(),
                Stroke::new(1.0, theme::BONE),
            );
            painter.hline(
                mark.center().x..=mark.right() - 4.0,
                mark.bottom() - 6.0,
                Stroke::new(1.0, theme::BLUE),
            );
            ui.add_space(12.0);
            theme::label(
                ui,
                "MODELVIEW",
                15.0,
                theme::narrow_semi(),
                theme::BONE_HI,
                3.0,
            );
            ui.add_space(22.0);
            vsep(ui, 32.0);
            ui.add_space(20.0);

            ui.vertical(|ui| {
                ui.add_space(12.0);
                let name = if self.display_name.is_empty() {
                    "No model open".to_string()
                } else {
                    truncate(&self.display_name, 42)
                };
                let name_color = match &self.phase {
                    Phase::Ready => theme::BONE_HI,
                    Phase::Failed { .. } => theme::RED,
                    _ => theme::DIM,
                };
                theme::label(ui, name, 14.0, theme::narrow_med(), name_color, 0.4);
                ui.add_space(1.0);
                theme::label(ui, self.file_meta(), 8.5, theme::narrow(), theme::DIM, 1.7);
            });

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(16.0);
                let open_response = if self.phase.is_ready() {
                    let mut job = egui::text::LayoutJob::default();
                    job.append(
                        "—  ",
                        0.0,
                        egui::TextFormat {
                            font_id: FontId::new(15.0, theme::narrow_med()),
                            color: theme::BLUE,
                            ..Default::default()
                        },
                    );
                    job.append(
                        "OPEN FILE",
                        0.0,
                        egui::TextFormat {
                            font_id: FontId::new(12.5, theme::narrow_semi()),
                            color: theme::BONE,
                            extra_letter_spacing: 1.6,
                            ..Default::default()
                        },
                    );
                    ui.add(
                        Button::new(job)
                            .stroke(Stroke::new(1.0, theme::BLUE))
                            .min_size(Vec2::new(140.0, 36.0)),
                    )
                } else {
                    ui.add(
                        Button::new(
                            RichText::new("OPEN FILE")
                                .font(FontId::new(13.0, theme::narrow_semi()))
                                .color(theme::BONE)
                                .extra_letter_spacing(1.7),
                        )
                        .fill(theme::BLUE)
                        .stroke(Stroke::NONE)
                        .min_size(Vec2::new(126.0, 36.0)),
                    )
                };
                focus_ring(ui, &open_response);
                if open_response.clicked() {
                    self.open_dialog(ui.ctx());
                }
                ui.add_space(14.0);
                if self.phase.is_ready() {
                    for preset in [
                        ViewPreset::Right,
                        ViewPreset::Front,
                        ViewPreset::Top,
                        ViewPreset::Home,
                    ] {
                        let active = self.active_view == Some(preset);
                        let color = if active { theme::BONE } else { theme::DIM };
                        let button = Button::new(
                            RichText::new(preset.label().to_uppercase())
                                .font(FontId::new(12.0, theme::narrow_semi()))
                                .color(color)
                                .extra_letter_spacing(1.4),
                        )
                        .stroke(Stroke::new(
                            1.0,
                            if active { theme::BLUE } else { theme::LINE2 },
                        ))
                        .min_size(Vec2::new(58.0, 34.0));
                        let button = if active {
                            button.fill(theme::ASH)
                        } else {
                            button
                        };
                        let response = ui.add(button);
                        focus_ring(ui, &response);
                        if active {
                            ui.painter().hline(
                                response.rect.x_range(),
                                response.rect.bottom() - 1.0,
                                Stroke::new(2.0, theme::BLUE),
                            );
                        }
                        if response.clicked() {
                            self.theater = None;
                            self.camera_flight = None;
                            self.camera.preset(preset);
                            self.active_view = Some(preset);
                        }
                    }
                }
            });
        });
        let rect = ui.max_rect();
        ui.painter().hline(
            rect.x_range(),
            rect.bottom() - 0.5,
            Stroke::new(1.0, theme::LINE),
        );
    }

    fn file_meta(&self) -> String {
        match (&self.phase, &self.scene) {
            (Phase::Ready, Some(scene)) => {
                if scene.format == Format::ThreeMf {
                    format!(
                        "3MF / {} OBJECTS / {} / {}",
                        scene.objects.len(),
                        if scene.plates.is_empty() {
                            "NO PLATES".to_string()
                        } else {
                            format!("{} PLATES", scene.plates.len())
                        },
                        theme::fmt_bytes(scene.byte_len)
                    )
                } else {
                    format!(
                        "STL / {} TRIANGLES / {}",
                        theme::fmt_int(self.summary.triangles),
                        theme::fmt_bytes(scene.byte_len)
                    )
                }
            }
            (Phase::Loading { name, .. }, _) if self.recovery_load => {
                format!("RECOVERING {name} / RESOLVING EXTERNAL PARTS")
            }
            (Phase::Loading { name, .. }, _) => self
                .stream
                .as_ref()
                .map(|stream| {
                    format!(
                        "STL / LARGE MODEL / {} OF {} TRIANGLES",
                        theme::fmt_int(stream.scan.triangles),
                        theme::fmt_int(stream.total)
                    )
                })
                .unwrap_or_else(|| format!("READING {name}")),
            (Phase::Failed { .. }, _) => "READ FAILED".to_string(),
            _ => "STL AND 3MF INSPECTION".to_string(),
        }
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        if self.sheet_clips_status(ui.ctx()) {
            return;
        }
        let (mut tag, mut color, mut message) = match &self.phase {
            Phase::Ready => {
                let verdict = self.verdict();
                let tag = match verdict {
                    Verdict::Watertight => "READY",
                    Verdict::Warning => "CHECK",
                    Verdict::Failed => "REPAIR",
                };
                let color = match verdict {
                    Verdict::Watertight => theme::BONE,
                    Verdict::Warning => theme::AMBER,
                    Verdict::Failed => theme::RED_TEXT,
                };
                (tag, color, self.status_headline())
            }
            Phase::Loading { name, .. } => {
                if self.recovery_load {
                    (
                        "RECOVER",
                        theme::AMBER,
                        format!("resolving external object parts for {name}"),
                    )
                } else if let Some(stream) = &self.stream {
                    (
                        if stream.verifying { "VERIFY" } else { "SCAN" },
                        theme::BLUE,
                        format!(
                            "{} / {} triangles · {} open edges so far",
                            theme::fmt_int(stream.scan.triangles),
                            theme::fmt_int(stream.total),
                            theme::fmt_int(self.scan_display[0].round() as usize)
                        ),
                    )
                } else {
                    (
                        "READING",
                        theme::BONE,
                        format!("parsing {name} and checking topology"),
                    )
                }
            }
            Phase::Failed { title, .. } => ("ERROR", theme::RED, title.clone()),
            Phase::Empty => (
                "NO FILE",
                theme::DIM,
                "drop an STL or 3MF to inspect it".to_string(),
            ),
        };
        if self.orientation_active() {
            tag = "ORIENT";
            color = theme::BLUE;
            message = format!(
                "bed 256 × 256 mm · {}",
                self.orientation
                    .as_ref()
                    .and_then(|orientation| orientation.selected)
                    .and_then(|index| self.orientation.as_ref()?.report.candidates.get(index))
                    .map(|candidate| format!("{:.1} mm high", candidate.height_mm))
                    .unwrap_or_else(|| "preview".to_string())
            );
        }
        if matches!(self.phase, Phase::Failed { .. }) && self.recovery_available {
            tag = "RECOVER";
            color = theme::AMBER;
            message = "external object parts can be fixed on load".to_string();
        }
        if let Some((text, at)) = &self.notice {
            if at.elapsed() < Duration::from_secs(8) {
                tag = "NOTICE";
                color = theme::BONE;
                message = text.clone();
            } else {
                self.notice = None;
            }
        }

        let triangles = self
            .stream
            .as_ref()
            .map(|stream| stream.scan.triangles)
            .unwrap_or(self.summary.triangles);

        ui.horizontal_centered(|ui| {
            ui.add_space(16.0);
            theme::label(ui, tag, 9.0, theme::narrow_semi(), color, 2.2);
            theme::label(ui, "—", 9.0, theme::narrow(), theme::FAINT, 1.0);
            theme::label(
                ui,
                truncate(&message, 64),
                9.0,
                theme::narrow(),
                theme::BONE,
                0.4,
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(16.0);
                theme::label(ui, "MM", 8.5, theme::narrow_semi(), theme::DIM, 1.5);
                vsep(ui, 16.0);
                theme::label(
                    ui,
                    format!("{} TRIANGLES", theme::fmt_int(triangles)),
                    8.5,
                    theme::narrow(),
                    theme::DIM,
                    1.5,
                );
                if let Some(stream) = &self.stream {
                    vsep(ui, 16.0);
                    theme::label(
                        ui,
                        format!(
                            "SCAN {}",
                            theme::fmt_ms(stream.started.elapsed().as_secs_f64() * 1000.0)
                        ),
                        8.5,
                        theme::narrow(),
                        theme::DIM,
                        1.5,
                    );
                } else if self.phase.is_ready() {
                    vsep(ui, 16.0);
                    theme::label(
                        ui,
                        format!("CHECKS {}", theme::fmt_ms(self.summary.check_ms)),
                        8.5,
                        theme::narrow(),
                        theme::DIM,
                        1.5,
                    );
                    vsep(ui, 16.0);
                    theme::label(
                        ui,
                        format!("PARSE {}", theme::fmt_ms(self.parse_ms)),
                        8.5,
                        theme::narrow(),
                        theme::DIM,
                        1.5,
                    );
                }
                vsep(ui, 16.0);
                let gpu = if self.gpu.is_some() {
                    self.gpu_name.clone()
                } else if self.stream.is_some() {
                    "LIVE GPU".to_string()
                } else if self.phase.is_ready() {
                    "UPLOADING".to_string()
                } else {
                    "GPU IDLE".to_string()
                };
                theme::label(
                    ui,
                    truncate(&gpu, 30),
                    8.5,
                    theme::narrow(),
                    theme::DIM,
                    1.5,
                );
            });
        });
        let rect = ui.max_rect();
        ui.painter().hline(
            rect.x_range(),
            rect.top() + 0.5,
            Stroke::new(1.0, theme::LINE),
        );
    }

    fn rail(&mut self, ui: &mut Ui) {
        let rect = ui.max_rect();
        let painter = ui.painter();
        painter.rect_filled(rect, egui::CornerRadius::ZERO, theme::PANEL);
        painter.vline(
            rect.right() - 0.5,
            rect.y_range(),
            Stroke::new(1.0, theme::LINE),
        );
        ptext(
            painter,
            Pos2::new(rect.center().x, rect.top() + 18.0),
            Align2::CENTER_CENTER,
            "INDEX",
            8.0,
            theme::narrow_semi(),
            theme::DIM,
            2.4,
        );

        let sections = self.section_labels();
        let mut y = rect.top() + 46.0;
        for (index, label) in sections.iter().enumerate() {
            let row = Rect::from_min_size(Pos2::new(rect.left(), y), Vec2::new(rect.width(), 58.0));
            let available = !label.is_empty();
            let response = if available {
                ui.interact(row, Id::unique(("rail", index)), Sense::click())
            } else {
                ui.interact(row, Id::unique(("rail", index)), Sense::hover())
            };
            let active = available && self.active_section == index;
            if available && response.hovered() {
                painter.rect_filled(row, egui::CornerRadius::ZERO, theme::ASH);
            }
            let color = if active {
                theme::BONE_HI
            } else if available {
                theme::DIM
            } else {
                theme::FAINT
            };
            if active {
                painter.hline(
                    row.x_range(),
                    row.bottom() - 0.5,
                    Stroke::new(1.0, theme::BLUE),
                );
            }
            let tick_width = if active {
                30.0
            } else if available {
                14.0
            } else {
                8.0
            };
            painter.hline(
                row.left()..=row.left() + tick_width,
                row.center().y,
                Stroke::new(
                    1.0,
                    if active {
                        theme::BLUE
                    } else if available {
                        theme::LINE2
                    } else {
                        theme::FAINT
                    },
                ),
            );
            ptext(
                painter,
                Pos2::new(rect.center().x, row.center().y - 7.0),
                Align2::CENTER_CENTER,
                format!("0{}", index + 1),
                10.0,
                theme::narrow_med(),
                color,
                1.4,
            );
            if available {
                ptext(
                    painter,
                    Pos2::new(rect.center().x, row.center().y + 8.0),
                    Align2::CENTER_CENTER,
                    *label,
                    7.0,
                    theme::narrow_semi(),
                    if active { theme::BONE } else { theme::DIM },
                    1.0,
                );
                focus_ring(ui, &response);
                if activated(ui, &response) {
                    self.active_section = index;
                    self.scroll_to = Some(index);
                    self.sidebar_open = true;
                }
                response.on_hover_cursor(CursorIcon::PointingHand);
            }
            y += 58.0;
        }

        let gear_rect = Rect::from_center_size(
            Pos2::new(rect.center().x - 19.0, rect.bottom() - 34.0),
            Vec2::splat(34.0),
        );
        let gear_response = ui.interact(gear_rect, Id::unique("rail_settings"), Sense::click());
        let gear_color = if self.settings_open {
            theme::BLUE
        } else if gear_response.hovered() {
            theme::BONE_HI
        } else {
            theme::DIM
        };
        painter.rect_stroke(
            gear_rect,
            egui::CornerRadius::ZERO,
            Stroke::new(
                1.0,
                if self.settings_open {
                    theme::BLUE
                } else {
                    theme::LINE2
                },
            ),
            StrokeKind::Inside,
        );
        painter.circle_stroke(gear_rect.center(), 6.5, Stroke::new(1.0, gear_color));
        for index in 0..8 {
            let angle = index as f32 * std::f32::consts::TAU / 8.0;
            let dir = Vec2::new(angle.cos(), angle.sin());
            painter.line_segment(
                [
                    gear_rect.center() + dir * 6.5,
                    gear_rect.center() + dir * 9.0,
                ],
                Stroke::new(1.0, gear_color),
            );
        }
        painter.circle_stroke(gear_rect.center(), 2.5, Stroke::new(1.0, gear_color));
        focus_ring(ui, &gear_response);
        if activated(ui, &gear_response) {
            self.settings_open = !self.settings_open;
            if self.settings_open {
                self.help_open = false;
            }
        }
        gear_response
            .on_hover_text("Slicer settings")
            .on_hover_cursor(CursorIcon::PointingHand);

        let help_rect = Rect::from_center_size(
            Pos2::new(rect.center().x + 19.0, rect.bottom() - 34.0),
            Vec2::splat(34.0),
        );
        let help_response = ui.interact(help_rect, Id::unique("rail_help"), Sense::click());
        let help_color = if help_response.hovered() {
            theme::BONE_HI
        } else {
            theme::DIM
        };
        painter.rect_stroke(
            help_rect,
            egui::CornerRadius::ZERO,
            Stroke::new(1.0, theme::LINE2),
            StrokeKind::Inside,
        );
        ptext(
            painter,
            help_rect.center(),
            Align2::CENTER_CENTER,
            "?",
            12.0,
            theme::narrow_med(),
            help_color,
            0.0,
        );
        focus_ring(ui, &help_response);
        if activated(ui, &help_response) {
            self.help_open = !self.help_open;
            if self.help_open {
                self.settings_open = false;
            }
        }
        help_response
            .on_hover_text("Controls and shortcuts")
            .on_hover_cursor(CursorIcon::PointingHand);
    }

    fn side_panel(&mut self, ui: &mut Ui) {
        if self.sheet_clips_inspector(ui.ctx()) {
            return;
        }
        let panel_rect = ui.max_rect();
        let mut marks: Vec<(f32, theme::StateMark)> = Vec::new();
        egui::ScrollArea::vertical()
            .id_salt("side_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().indent = 48.0;
                ui.visuals_mut().indent_has_left_vline = false;
                ui.indent("side_content", |ui| {
                    ui.set_width((ui.available_width() - 24.0).max(200.0));
                    ui.add_space(18.0);
                    self.side_content(ui, &mut marks);
                    ui.add_space(28.0);
                });
            });

        let painter = ui.painter();
        painter.vline(
            panel_rect.left() + 28.0,
            panel_rect.y_range(),
            Stroke::new(1.0, theme::LINE),
        );
        for (y, mark) in marks {
            theme::state_mark(painter, Pos2::new(panel_rect.left() + 28.0, y), mark);
            painter.hline(
                panel_rect.left() + 39.0..=panel_rect.left() + 44.0,
                y,
                Stroke::new(1.0, theme::FAINT),
            );
        }
        painter.vline(
            panel_rect.left() + 0.5,
            panel_rect.y_range(),
            Stroke::new(1.0, theme::LINE),
        );
    }

    fn side_content(&mut self, ui: &mut Ui, marks: &mut Vec<(f32, theme::StateMark)>) {
        if self.scroll_to == Some(0) {
            ui.scroll_to_cursor(Some(Align::TOP));
            self.scroll_to = None;
        }
        let phase = self.phase.clone();
        match phase {
            Phase::Ready => {
                let verdict = self.verdict();
                let body = match verdict {
                    Verdict::Watertight => {
                        "Every edge joins two faces and all normals agree. Ready for the slicer."
                    }
                    Verdict::Warning => {
                        "Topology is closed, but there is clean-up worth doing before slicing."
                    }
                    Verdict::Failed => "The mesh has holes or bad joins. Repair it before slicing.",
                };
                self.verdict_block(
                    ui,
                    verdict.label().to_uppercase(),
                    theme::verdict_color(verdict),
                    format!(
                        "{} / {} CHECKS / {}",
                        match verdict {
                            Verdict::Watertight => "PASS",
                            Verdict::Warning => "CHECK",
                            Verdict::Failed => "REPAIR",
                        },
                        6,
                        theme::fmt_volume(self.summary.volume_mm3)
                    ),
                    body,
                    marks,
                );
                self.verdict_actions(ui, verdict);
                self.section_head(ui, 1, "CHECKS", Some("6 RUN"));
                self.checks_block(ui, marks);
                let multi = self
                    .scene
                    .as_ref()
                    .map(|scene| scene.objects.len() > 1 || !scene.plates.is_empty())
                    .unwrap_or(false);
                let count = self.scene.as_ref().map(|s| s.objects.len()).unwrap_or(0);
                if multi {
                    self.section_head(ui, 2, "OBJECTS", Some(&format!("{count}")));
                    self.objects_block(ui);
                    self.section_head(ui, 3, "MODEL", Some("MM"));
                    self.model_block(ui);
                } else {
                    self.section_head(ui, 2, "MODEL", Some("MM"));
                    self.model_block(ui);
                    self.section_head(ui, 3, "OBJECTS", Some(&format!("{count}")));
                    self.objects_block(ui);
                }
                self.section_head(ui, 4, "FILE", None);
                self.file_block(ui);
                self.section_head(ui, 5, "ORIENTATION", Some("BED 256 MM"));
                self.orientation_block(ui);
            }
            Phase::Empty => {
                self.verdict_block(
                    ui,
                    "NO SPECIMEN".to_string(),
                    theme::BONE,
                    "0 CHECKS / AWAITING FILE".to_string(),
                    "Drop an STL or 3MF to inspect its geometry.",
                    marks,
                );
                self.section_head(ui, 1, "CHECKS", Some("NOT RUN"));
                self.pending_checks(ui, marks);
                self.section_head(ui, 2, "MODEL", None);
                self.placeholder_stats(ui);
                self.section_head(ui, 3, "OBJECTS", None);
                self.placeholder_note(ui, "No objects yet.");
                self.section_head(ui, 4, "FILE", None);
                self.placeholder_note(
                    ui,
                    "STL binary or ASCII. 3MF with build items, plates, and metadata.",
                );
            }
            Phase::Loading { name, .. } => {
                if self.recovery_load {
                    self.verdict_block(
                        ui,
                        "RECOVERING".to_string(),
                        theme::AMBER,
                        "RESOLVING EXTERNAL OBJECT PARTS".to_string(),
                        "Flattening external model parts and preserving every plate. The source file is not modified.",
                        marks,
                    );
                    self.section_head(ui, 1, "CHECKS", Some("RECOVERY"));
                    self.pending_checks(ui, marks);
                    self.section_head(ui, 4, "FILE", Some("RECOVERY"));
                    self.placeholder_note(
                        ui,
                        &format!("{name} · large 3MF package · reading external parts"),
                    );
                } else if let Some(stream) = &self.stream {
                    let total = stream.total;
                    let seen = stream.scan.triangles;
                    let verifying = stream.verifying;
                    let open_edges = self.scan_display[0].round() as usize;
                    self.verdict_block(
                        ui,
                        if verifying {
                            "VERIFYING".to_string()
                        } else {
                            "STREAMING".to_string()
                        },
                        theme::BLUE,
                        format!(
                            "{} / {} TRIANGLES / {} OPEN SO FAR",
                            theme::fmt_int(seen),
                            theme::fmt_int(total),
                            theme::fmt_int(open_edges)
                        ),
                        "The mesh is already live. The filmstrip settles as each check reaches its result.",
                        marks,
                    );
                    self.section_head(
                        ui,
                        1,
                        "CHECKS",
                        Some(if verifying { "VERIFY" } else { "LIVE" }),
                    );
                    self.scan_checks(ui, marks);
                    self.section_head(ui, 4, "FILE", Some("LARGE MODEL"));
                    self.placeholder_note(
                        ui,
                        &format!(
                            "{name} · binary STL stream · {} of {} triangles",
                            theme::fmt_int(seen),
                            theme::fmt_int(total)
                        ),
                    );
                } else {
                    self.verdict_block(
                        ui,
                        "READING".to_string(),
                        theme::BONE,
                        "PARSING + CHECKING".to_string(),
                        "Geometry is being read and measured on the CPU. The viewport stays free.",
                        marks,
                    );
                    self.section_head(ui, 1, "CHECKS", Some("RUNNING"));
                    self.pending_checks(ui, marks);
                    self.section_head(ui, 4, "FILE", Some("LOADING"));
                    self.placeholder_note(ui, &name);
                }
            }
            Phase::Failed { title, message } => {
                self.verdict_block(
                    ui,
                    "READ FAILED".to_string(),
                    theme::RED,
                    "0 CHECKS / NO GEOMETRY".to_string(),
                    &title,
                    marks,
                );
                self.section_head(ui, 1, "CHECKS", Some("NOT RUN"));
                self.pending_checks(ui, marks);
                self.section_head(ui, 4, "DETAIL", None);
                ui.label(
                    RichText::new(message.as_str())
                        .font(FontId::new(14.5, egui::FontFamily::Proportional))
                        .color(theme::BONE),
                );
                ui.add_space(8.0);
            }
        }
    }

    fn verdict_block(
        &self,
        ui: &mut Ui,
        title: String,
        color: Color32,
        state_line: String,
        body: &str,
        marks: &mut Vec<(f32, theme::StateMark)>,
    ) {
        ui.add_space(2.0);
        let width = ui.available_width();
        let size = 41.0;
        let galley = ui.painter().layout_job(theme::text_job(
            title,
            size,
            theme::narrow_med(),
            color,
            2.2,
        ));
        let (rect, _) = ui.allocate_exact_size(galley.size(), Sense::hover());
        ui.painter().galley(rect.min, galley, color);
        ui.add_space(4.0);
        theme::label(ui, state_line, 9.0, theme::narrow_semi(), theme::DIM, 1.8);
        ui.add_space(6.0);
        ui.label(
            RichText::new(body)
                .font(FontId::new(14.5, egui::FontFamily::Proportional))
                .color(theme::BONE),
        );
        ui.add_space(8.0);
        let (rule, _) = ui.allocate_exact_size(Vec2::new(width, 2.0), Sense::hover());
        ui.painter()
            .rect_filled(rule, egui::CornerRadius::ZERO, color);
        marks.push((
            rule.center().y,
            match &self.phase {
                Phase::Ready => match self.verdict() {
                    Verdict::Watertight => theme::StateMark::Pass,
                    Verdict::Warning => theme::StateMark::Warn,
                    Verdict::Failed => theme::StateMark::Fail,
                },
                Phase::Failed { .. } => theme::StateMark::Fail,
                Phase::Loading { .. } => theme::StateMark::Warn,
                Phase::Empty => theme::StateMark::Info,
            },
        ));
        ui.add_space(2.0);
    }

    fn section_head(&mut self, ui: &mut Ui, index: usize, title: &str, meta: Option<&str>) {
        if self.scroll_to == Some(index) {
            ui.scroll_to_cursor(Some(Align::TOP));
            self.scroll_to = None;
        }
        ui.add_space(18.0);
        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 18.0), Sense::hover());
        let painter = ui.painter();
        ptext(
            painter,
            rect.left_top(),
            Align2::LEFT_TOP,
            format!("0{} / {title}", index + 1),
            8.5,
            theme::narrow_semi(),
            theme::DIM,
            2.2,
        );
        if let Some(meta) = meta {
            ptext(
                painter,
                rect.right_top(),
                Align2::RIGHT_TOP,
                meta,
                8.5,
                theme::narrow(),
                theme::DIM,
                1.5,
            );
        }
        painter.hline(
            rect.x_range(),
            rect.bottom() - 0.5,
            Stroke::new(1.0, theme::LINE),
        );
        ui.add_space(8.0);
    }

    fn checks_block(&mut self, ui: &mut Ui, marks: &mut Vec<(f32, theme::StateMark)>) {
        let rows = self.check_rows();
        let width = ui.available_width();
        for (index, row) in rows.into_iter().enumerate() {
            let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 46.0), Sense::click());
            if response.hovered() {
                ui.painter().rect_filled(
                    rect,
                    egui::CornerRadius::ZERO,
                    theme::ASH.gamma_multiply(0.7),
                );
            }
            let value_color = match row.state {
                theme::StateMark::Fail => theme::RED_TEXT,
                theme::StateMark::Warn => theme::AMBER,
                theme::StateMark::Pass => theme::BONE_HI,
                theme::StateMark::Info => theme::DIM,
            };
            ptext(
                ui.painter(),
                Pos2::new(rect.left(), rect.top() + 8.0),
                Align2::LEFT_TOP,
                row.label,
                12.0,
                theme::narrow_med(),
                theme::BONE,
                0.2,
            );
            ptext(
                ui.painter(),
                Pos2::new(rect.left(), rect.top() + 28.0),
                Align2::LEFT_TOP,
                row.detail,
                9.5,
                theme::narrow(),
                theme::DIM,
                0.2,
            );
            ptext(
                ui.painter(),
                Pos2::new(rect.right(), rect.top() + 10.0),
                Align2::RIGHT_TOP,
                row.value,
                10.5,
                theme::narrow_semi(),
                value_color,
                1.0,
            );
            marks.push((rect.top() + 18.0, row.state));
            if self.focus_demo && index == 0 {
                response.request_focus();
                draw_focus_ring(ui, rect);
            }
            focus_ring(ui, &response);
            if response.has_focus() {
                ui.scroll_to_rect(rect, Some(Align::Center));
            }
            if activated(ui, &response)
                && let Some(kind) = row.kind
            {
                self.focus_kind(kind);
            }
            response.on_hover_cursor(if row.kind.is_some() {
                CursorIcon::PointingHand
            } else {
                CursorIcon::Default
            });
            ui.painter().hline(
                rect.x_range(),
                rect.bottom() - 0.5,
                Stroke::new(1.0, theme::LINE),
            );
        }
    }

    fn scan_checks(&self, ui: &mut Ui, marks: &mut Vec<(f32, theme::StateMark)>) {
        let rows = [
            (
                "Closed surface",
                "open edges so far",
                self.scan_display[0],
                true,
            ),
            (
                "Manifold joins",
                "non-manifold so far",
                self.scan_display[1],
                true,
            ),
            ("Winding", "inconsistent so far", self.scan_display[2], true),
            ("Volume", "signed, live", self.scan_display[3], false),
            ("Shells", "connected, live", self.scan_display[4], false),
            (
                "Degenerate faces",
                "zero-area so far",
                self.scan_display[5],
                true,
            ),
        ];
        let width = ui.available_width();
        for (label, detail, value, count) in rows {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 42.0), Sense::hover());
            ptext(
                ui.painter(),
                Pos2::new(rect.left(), rect.top() + 6.0),
                Align2::LEFT_TOP,
                label,
                12.0,
                theme::narrow_med(),
                theme::BONE,
                0.2,
            );
            ptext(
                ui.painter(),
                Pos2::new(rect.left(), rect.top() + 25.0),
                Align2::LEFT_TOP,
                detail,
                9.0,
                theme::narrow(),
                theme::FAINT,
                0.2,
            );
            let text = if count {
                theme::fmt_int(value.round() as usize)
            } else if label == "Volume" {
                theme::fmt_volume(value as f64)
            } else {
                theme::fmt_int(value.round() as usize)
            };
            ptext(
                ui.painter(),
                Pos2::new(rect.right(), rect.top() + 8.0),
                Align2::RIGHT_TOP,
                text,
                10.0,
                theme::narrow_semi(),
                theme::DIM,
                1.0,
            );
            marks.push((rect.top() + 16.0, theme::StateMark::Info));
            ui.painter().hline(
                rect.x_range(),
                rect.bottom() - 0.5,
                Stroke::new(1.0, theme::LINE),
            );
        }
    }

    fn pending_checks(&self, ui: &mut Ui, marks: &mut Vec<(f32, theme::StateMark)>) {
        let rows = self.check_rows();
        let width = ui.available_width();
        for row in rows.iter() {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 46.0), Sense::hover());
            ptext(
                ui.painter(),
                Pos2::new(rect.left(), rect.top() + 8.0),
                Align2::LEFT_TOP,
                row.label,
                12.0,
                theme::narrow_med(),
                theme::DIM,
                0.2,
            );
            ptext(
                ui.painter(),
                Pos2::new(rect.left(), rect.top() + 28.0),
                Align2::LEFT_TOP,
                row.detail,
                9.5,
                theme::narrow(),
                theme::DIM,
                0.2,
            );
            ptext(
                ui.painter(),
                Pos2::new(rect.right(), rect.top() + 10.0),
                Align2::RIGHT_TOP,
                "—",
                10.5,
                theme::narrow_semi(),
                theme::FAINT,
                1.0,
            );
            marks.push((rect.top() + 18.0, theme::StateMark::Info));
            ui.painter().hline(
                rect.x_range(),
                rect.bottom() - 0.5,
                Stroke::new(1.0, theme::LINE),
            );
        }
    }

    fn model_block(&self, ui: &mut Ui) {
        let bounds = self
            .scene
            .as_ref()
            .map(|s| s.bounds())
            .unwrap_or_else(Bounds::empty);
        let width = ui.available_width();
        let (row, _) = ui.allocate_exact_size(Vec2::new(width, 54.0), Sense::hover());
        ptext(
            ui.painter(),
            row.left_top(),
            Align2::LEFT_TOP,
            "BOUNDING BOX",
            8.0,
            theme::narrow_semi(),
            theme::DIM,
            1.6,
        );
        let dims = if bounds.is_empty() {
            "—".to_string()
        } else {
            let s = bounds.size();
            format!("{:.1} × {:.1} × {:.1} mm", s.x, s.y, s.z)
        };
        ptext(
            ui.painter(),
            Pos2::new(row.left(), row.top() + 22.0),
            Align2::LEFT_TOP,
            dims,
            15.0,
            theme::narrow_med(),
            theme::BONE_HI,
            0.4,
        );
        ui.painter().hline(
            row.x_range(),
            row.bottom() - 0.5,
            Stroke::new(1.0, theme::LINE),
        );

        let pairs = [
            (
                "TRIANGLES",
                if self.summary.triangles == 0 {
                    "—".to_string()
                } else {
                    theme::fmt_int(self.summary.triangles)
                },
            ),
            (
                "VERTICES",
                if self.summary.vertices == 0 {
                    "—".to_string()
                } else {
                    theme::fmt_int(self.summary.vertices)
                },
            ),
            (
                "VOLUME",
                if self.summary.triangles == 0 {
                    "—".to_string()
                } else {
                    theme::fmt_volume(self.summary.volume_mm3)
                },
            ),
            (
                "SURFACE",
                if self.summary.triangles == 0 {
                    "—".to_string()
                } else {
                    theme::fmt_area(self.summary.surface_area_mm2)
                },
            ),
            (
                "SHELLS",
                if self.summary.triangles == 0 {
                    "—".to_string()
                } else {
                    theme::fmt_int(self.summary.shells)
                },
            ),
            (
                "OPEN EDGES",
                if self.summary.triangles == 0 {
                    "—".to_string()
                } else {
                    theme::fmt_int(self.summary.open_edges)
                },
            ),
        ];
        for chunk in pairs.chunks(2) {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 48.0), Sense::hover());
            let half = width / 2.0;
            for (offset, (label, value)) in chunk.iter().enumerate() {
                let x = rect.left() + offset as f32 * half;
                ptext(
                    ui.painter(),
                    Pos2::new(x, rect.top() + 7.0),
                    Align2::LEFT_TOP,
                    *label,
                    8.0,
                    theme::narrow_semi(),
                    theme::DIM,
                    1.5,
                );
                ptext(
                    ui.painter(),
                    Pos2::new(x, rect.top() + 22.0),
                    Align2::LEFT_TOP,
                    value,
                    15.0,
                    theme::narrow_med(),
                    theme::BONE,
                    0.3,
                );
            }
            ui.painter().hline(
                rect.x_range(),
                rect.bottom() - 0.5,
                Stroke::new(1.0, theme::LINE),
            );
        }
    }

    fn placeholder_stats(&self, ui: &mut Ui) {
        let width = ui.available_width();
        for label in ["BOUNDING BOX", "TRIANGLES", "VOLUME", "SURFACE"] {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 44.0), Sense::hover());
            ptext(
                ui.painter(),
                Pos2::new(rect.left(), rect.top() + 10.0),
                Align2::LEFT_TOP,
                label,
                8.5,
                theme::narrow_semi(),
                theme::DIM,
                1.6,
            );
            ptext(
                ui.painter(),
                Pos2::new(rect.right(), rect.top() + 8.0),
                Align2::RIGHT_TOP,
                "—",
                10.0,
                theme::narrow(),
                theme::FAINT,
                1.0,
            );
            ui.painter().hline(
                rect.x_range(),
                rect.bottom() - 0.5,
                Stroke::new(1.0, theme::LINE),
            );
        }
    }

    fn placeholder_note(&self, ui: &mut Ui, text: &str) {
        ui.add_space(6.0);
        ui.label(
            RichText::new(text)
                .font(FontId::new(14.5, egui::FontFamily::Proportional))
                .color(theme::DIM),
        );
        ui.add_space(6.0);
    }

    fn objects_block(&mut self, ui: &mut Ui) {
        let Some(scene) = &self.scene else {
            ui.add_space(6.0);
            ui.label(
                RichText::new("No objects yet.")
                    .font(FontId::new(14.5, egui::FontFamily::Proportional))
                    .color(theme::DIM),
            );
            return;
        };

        let plate_names: std::collections::HashMap<u32, String> = scene
            .plates
            .iter()
            .map(|p| (p.id, p.name.clone()))
            .collect();
        let plate_bounds: std::collections::HashMap<u32, Bounds> = scene
            .plates
            .iter()
            .map(|plate| {
                let mut bounds = Bounds::empty();
                for index in &plate.objects {
                    if let Some(object) = scene.objects.get(*index) {
                        bounds.union(&object.mesh.bounds());
                    }
                }
                (plate.id, bounds)
            })
            .collect();

        let mut order: Vec<usize> = (0..scene.objects.len()).collect();
        order.sort_by_key(|i| (scene.objects[*i].plate.unwrap_or(0), *i));
        let mut last_plate = None;
        let width = ui.available_width();
        for index in order {
            let object = &scene.objects[index];
            if object.plate != last_plate {
                if let Some(plate) = object.plate {
                    let label = plate_names
                        .get(&plate)
                        .cloned()
                        .unwrap_or_else(|| format!("Plate {plate}"));
                    let bounds = plate_bounds.get(&plate).cloned().unwrap_or_default();
                    let active = self
                        .selection
                        .and_then(|i| scene.objects.get(i))
                        .map(|o| o.plate == Some(plate))
                        .unwrap_or(false);
                    ui.add_space(6.0);
                    let button = Button::new(
                        RichText::new(format!("PLATE {:02}", plate))
                            .font(FontId::new(11.5, theme::narrow_semi()))
                            .color(theme::BONE)
                            .extra_letter_spacing(1.4),
                    )
                    .stroke(Stroke::new(
                        1.0,
                        if active { theme::BLUE } else { theme::LINE2 },
                    ))
                    .min_size(Vec2::new(90.0, 30.0));
                    let button = if active {
                        button.fill(theme::BLUE_DIM)
                    } else {
                        button
                    };
                    let response = ui.add(button);
                    focus_ring(ui, &response);
                    if response.clicked() {
                        self.theater = None;
                        self.camera_flight = None;
                        self.camera.fit(&bounds);
                        self.active_view = None;
                    }
                    response.on_hover_text(format!("{label} — click to frame"));
                    ui.add_space(4.0);
                }
                last_plate = object.plate;
            }
            let selected = self.selection == Some(index);
            let visible = self.visible.get(index).copied().unwrap_or(true);
            let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 54.0), Sense::click());
            let square = Rect::from_center_size(
                Pos2::new(rect.left() + 8.0, rect.center().y),
                Vec2::splat(14.0),
            );
            let vis_response = ui.interact(
                square.expand(9.0),
                Id::unique(("visibility", index)),
                Sense::click(),
            );
            let vis_clicked = vis_response.clicked();

            if selected {
                ui.painter()
                    .rect_filled(rect, egui::CornerRadius::ZERO, theme::ASH);
                ui.painter().hline(
                    rect.x_range(),
                    rect.bottom() - 0.5,
                    Stroke::new(1.0, theme::BLUE),
                );
            } else if response.hovered() {
                ui.painter().rect_filled(
                    rect,
                    egui::CornerRadius::ZERO,
                    theme::ASH.gamma_multiply(0.55),
                );
            }
            ui.painter().rect_stroke(
                square,
                egui::CornerRadius::ZERO,
                Stroke::new(1.0, if visible { theme::BONE } else { theme::LINE2 }),
                StrokeKind::Inside,
            );
            if visible {
                ui.painter()
                    .rect_filled(square.shrink(3.0), egui::CornerRadius::ZERO, theme::BONE);
            }
            let text_left = if let Some(color) = object.color {
                let swatch = Rect::from_center_size(
                    Pos2::new(rect.left() + 34.0, rect.center().y),
                    Vec2::splat(10.0),
                );
                ui.painter().rect_filled(
                    swatch,
                    egui::CornerRadius::ZERO,
                    Color32::from_rgb(color[0], color[1], color[2]),
                );
                ui.painter().rect_stroke(
                    swatch,
                    egui::CornerRadius::ZERO,
                    Stroke::new(1.0, theme::LINE2),
                    StrokeKind::Inside,
                );
                rect.left() + 46.0
            } else {
                rect.left() + 28.0
            };
            ptext(
                ui.painter(),
                Pos2::new(text_left, rect.top() + 10.0),
                Align2::LEFT_TOP,
                truncate(&object.name, 30),
                12.0,
                theme::narrow_med(),
                if selected {
                    theme::BONE_HI
                } else {
                    theme::BONE
                },
                0.2,
            );
            ptext(
                ui.painter(),
                Pos2::new(text_left, rect.top() + 31.0),
                Align2::LEFT_TOP,
                format!("{} TRIANGLES", theme::fmt_int(object.mesh.triangle_count())),
                8.0,
                theme::narrow(),
                theme::DIM,
                1.1,
            );
            let defects = self
                .reports
                .get(index)
                .map(|r| {
                    r.open_edges
                        + r.non_manifold_edges
                        + r.non_manifold_vertices
                        + r.inconsistent_edges
                })
                .unwrap_or(0);
            if defects > 0 {
                ptext(
                    ui.painter(),
                    Pos2::new(rect.right(), rect.center().y - 4.0),
                    Align2::RIGHT_CENTER,
                    format!("{} DEFECTS", theme::fmt_int(defects)),
                    8.5,
                    theme::narrow_semi(),
                    theme::RED_TEXT,
                    1.2,
                );
            } else if self.reports.get(index).is_some() {
                ptext(
                    ui.painter(),
                    Pos2::new(rect.right(), rect.center().y - 4.0),
                    Align2::RIGHT_CENTER,
                    "CLOSED",
                    8.5,
                    theme::narrow_semi(),
                    theme::DIM,
                    1.2,
                );
            }
            focus_ring(ui, &response);
            if response.has_focus() || vis_response.has_focus() {
                ui.scroll_to_rect(rect, Some(Align::Center));
            }
            if vis_response.has_focus() {
                ui.painter().rect_stroke(
                    square.expand(2.0),
                    egui::CornerRadius::ZERO,
                    Stroke::new(1.0, theme::BLUE),
                    StrokeKind::Inside,
                );
            }
            if vis_clicked || activated(ui, &vis_response) {
                if let Some(value) = self.visible.get_mut(index) {
                    *value = !*value;
                    if !*value && self.theater.is_some_and(|theater| theater.object == index) {
                        self.theater = None;
                        self.camera_flight = None;
                    }
                    self.gpu_defects_dirty = true;
                }
            } else if activated(ui, &response) {
                self.theater = None;
                self.camera_flight = None;
                self.selection = if selected { None } else { Some(index) };
            }
            response.on_hover_cursor(CursorIcon::PointingHand);
            vis_response
                .on_hover_text(if visible {
                    "Hide this object"
                } else {
                    "Show this object"
                })
                .on_hover_cursor(CursorIcon::PointingHand);
            ui.painter().hline(
                rect.x_range(),
                rect.bottom() - 0.5,
                Stroke::new(1.0, theme::LINE),
            );
        }
    }

    fn orientation_block(&mut self, ui: &mut Ui) {
        let job = self.orientation_job.is_some();
        let error = self.orientation_error.clone();
        let report = self.orientation.as_ref().map(|state| state.report.clone());
        let selected = self.orientation.as_ref().and_then(|state| state.selected);
        let has_pose = self.orientation_pose.is_some();

        ui.add_space(8.0);
        theme::label(
            ui,
            "Test candidate poses on a 256 × 256 × 256 mm bed. Preview only; the slicer applies the final pose.",
            10.5,
            theme::narrow(),
            theme::DIM,
            0.4,
        );
        ui.add_space(10.0);

        if let Some(error) = &error {
            theme::label(
                ui,
                "ORIENTATION ERROR",
                9.5,
                theme::narrow_semi(),
                theme::RED_TEXT,
                1.6,
            );
            ui.label(
                RichText::new(error)
                    .font(FontId::new(13.0, egui::FontFamily::Proportional))
                    .color(theme::RED_TEXT),
            );
            ui.add_space(8.0);
        }

        if job {
            theme::label(
                ui,
                "ANALYSING CANDIDATE POSES",
                9.5,
                theme::narrow_semi(),
                theme::BLUE,
                1.8,
            );
            ui.add_space(8.0);
            let width = ui.available_width();
            let (bar, _) = ui.allocate_exact_size(Vec2::new(width, 2.0), Sense::hover());
            ui.painter()
                .hline(bar.x_range(), bar.center().y, Stroke::new(1.0, theme::LINE));
            let phase = (ui.input(|i| i.time) as f32 * 0.55).fract();
            let x = bar.left() + (bar.width() - 72.0) * phase;
            ui.painter()
                .hline(x..=x + 72.0, bar.center().y, Stroke::new(2.0, theme::BLUE));
            ui.add_space(10.0);
            return;
        }

        let mut pick = None;
        let mut action = None;
        if let Some(report) = &report {
            let (verdict, verdict_color) = if report.fits_any {
                ("FITS BED", theme::BONE)
            } else {
                ("NO BED FIT", theme::RED_TEXT)
            };
            theme::label(ui, verdict, 11.0, theme::narrow_semi(), verdict_color, 1.6);
            if let Some(best) = report.candidates.first() {
                ui.add_space(2.0);
                theme::label(
                    ui,
                    format!(
                        "{} / H {:.1} MM / FOOT {:.0} × {:.0} MM",
                        truncate(&best.name, 34),
                        best.height_mm,
                        best.footprint_mm[0],
                        best.footprint_mm[1]
                    ),
                    9.0,
                    theme::narrow(),
                    theme::DIM,
                    0.4,
                );
            }
            ui.add_space(8.0);
            for (index, candidate) in report.candidates.iter().enumerate() {
                let width = ui.available_width();
                let (rect, response) =
                    ui.allocate_exact_size(Vec2::new(width, 58.0), Sense::click());
                let is_selected = selected == Some(index);
                if is_selected {
                    ui.painter()
                        .rect_filled(rect, egui::CornerRadius::ZERO, theme::ASH);
                } else if response.hovered() {
                    ui.painter().rect_filled(
                        rect,
                        egui::CornerRadius::ZERO,
                        theme::ASH.gamma_multiply(0.7),
                    );
                }
                if is_selected {
                    ui.painter().rect_stroke(
                        rect,
                        egui::CornerRadius::ZERO,
                        Stroke::new(1.0, theme::BLUE),
                        StrokeKind::Inside,
                    );
                }
                ptext(
                    ui.painter(),
                    Pos2::new(rect.left() + 8.0, rect.top() + 6.0),
                    Align2::LEFT_TOP,
                    truncate(&candidate.name, 38),
                    10.5,
                    theme::narrow_semi(),
                    theme::BONE,
                    0.5,
                );
                ptext(
                    ui.painter(),
                    Pos2::new(rect.left() + 8.0, rect.top() + 23.0),
                    Align2::LEFT_TOP,
                    format!(
                        "H {:.1} MM / {:.0} × {:.0} FOOT",
                        candidate.height_mm, candidate.footprint_mm[0], candidate.footprint_mm[1]
                    ),
                    8.5,
                    theme::narrow(),
                    theme::DIM,
                    0.3,
                );
                ptext(
                    ui.painter(),
                    Pos2::new(rect.left() + 8.0, rect.top() + 38.0),
                    Align2::LEFT_TOP,
                    format!(
                        "{} OVERHANG / {} CONTACT",
                        theme::fmt_area(candidate.overhang_mm2),
                        theme::fmt_area(candidate.contact_mm2)
                    ),
                    8.5,
                    theme::narrow(),
                    theme::DIM,
                    0.2,
                );
                ptext(
                    ui.painter(),
                    Pos2::new(rect.right() - 8.0, rect.top() + 7.0),
                    Align2::RIGHT_TOP,
                    if candidate.fits_bed { "FITS" } else { "OVER" },
                    8.0,
                    theme::narrow_semi(),
                    if candidate.fits_bed {
                        theme::BONE
                    } else {
                        theme::RED_TEXT
                    },
                    1.2,
                );
                focus_ring(ui, &response);
                if activated(ui, &response) {
                    pick = Some(index);
                }
                response.on_hover_cursor(CursorIcon::PointingHand);
                ui.painter().hline(
                    rect.x_range(),
                    rect.bottom() - 0.5,
                    Stroke::new(1.0, theme::LINE),
                );
            }
            if let Some(index) = pick {
                self.select_orientation(index, ui.ctx());
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let width = (ui.available_width() - 8.0) * 0.5;
                if sheet_button(ui, "RESET POSE", width, false) && has_pose {
                    action = Some(OrientationAction::Reset);
                }
                if sheet_button(ui, "RE-ANALYSE", width, false) {
                    action = Some(OrientationAction::Analyse);
                }
            });
            theme::label(
                ui,
                "PREVIEW ONLY / THE SLICER APPLIES THE ORIENTATION",
                8.0,
                theme::narrow_semi(),
                theme::DIM,
                1.4,
            );
        } else {
            let width = ui.available_width();
            if sheet_button(ui, "FIND BEST ORIENTATION", width, true) {
                action = Some(OrientationAction::Analyse);
            }
            ui.add_space(8.0);
            theme::label(
                ui,
                "No pose tested yet. The camera and grading stay on the CPU worker.",
                9.5,
                theme::narrow(),
                theme::DIM,
                0.3,
            );
        }

        if action == Some(OrientationAction::Analyse) {
            self.start_orientation(ui.ctx());
        } else if action == Some(OrientationAction::Reset) {
            self.reset_orientation_pose(ui.ctx());
        }
    }

    fn file_block(&mut self, ui: &mut Ui) {
        let Some(scene) = &self.scene else {
            ui.add_space(6.0);
            ui.label(
                RichText::new("No file open.")
                    .font(FontId::new(14.5, egui::FontFamily::Proportional))
                    .color(theme::DIM),
            );
            return;
        };
        let assumed = if scene.assumed_mm {
            "mm (assumed)"
        } else if scene.unit.mm() == 1.0 {
            "mm"
        } else {
            scene.unit.label()
        };
        let rows: Vec<(String, String)> = vec![
            ("FORMAT".to_string(), scene.format.label().to_string()),
            ("UNITS".to_string(), assumed.to_string()),
            ("SIZE".to_string(), theme::fmt_bytes(scene.byte_len)),
            ("PARSE".to_string(), theme::fmt_ms(self.parse_ms)),
            ("ANALYSIS".to_string(), theme::fmt_ms(self.summary.check_ms)),
            (
                "UPLOAD".to_string(),
                if self.upload_ms > 0.0 {
                    theme::fmt_ms(self.upload_ms)
                } else {
                    "pending".to_string()
                },
            ),
            (
                "SOURCE".to_string(),
                self.file_path
                    .as_ref()
                    .map(|p| truncate(&p.display().to_string(), 34))
                    .unwrap_or_else(|| "Built-in sample".to_string()),
            ),
        ];
        let width = ui.available_width();
        for (key, value) in rows {
            kv_row(ui, width, &key, &value);
        }

        if let Some(project) = &scene.project {
            ui.add_space(12.0);
            if let Some(application) = &project.application {
                kv_row(ui, width, "APPLICATION", application);
            }
            if let Some(printer) = &project.printer {
                kv_row(ui, width, "PRINTER", printer);
            }
            if !project.filaments.is_empty() {
                kv_row(ui, width, "FILAMENT", &project.filaments.join(" / "));
            }
        }

        if !scene.warnings.is_empty() {
            ui.add_space(12.0);
            for warning in &scene.warnings {
                let color = theme::BLUE;
                let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 36.0), Sense::hover());
                ui.painter().hline(
                    rect.left()..=rect.left() + 9.0,
                    rect.center().y,
                    Stroke::new(1.0, color),
                );
                ptext(
                    ui.painter(),
                    Pos2::new(rect.left() + 16.0, rect.center().y),
                    Align2::LEFT_CENTER,
                    truncate(warning, 48),
                    9.0,
                    theme::narrow(),
                    color,
                    0.2,
                );
            }
        }
    }

    fn viewport(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        let (rect, response) =
            ui.allocate_exact_size(ui.available_size(), Sense::CLICK | Sense::DRAG);
        self.viewport_rect = Some(rect);
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, egui::CornerRadius::ZERO, theme::VOID);

        let phase = self.phase.clone();
        match phase {
            Phase::Empty => self.empty_state(ui, rect),
            Phase::Loading { name, started } => {
                if self.stream.is_some() {
                    self.live_viewport(ui, rect, &response, frame);
                } else {
                    self.loading_state(ui, rect, &name, started);
                }
            }
            Phase::Ready => self.ready_viewport(ui, rect, &response, frame),
            Phase::Failed { title, .. } => {
                self.error_state(ui, rect, &title);
            }
        }

        let hovering_files = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
        if hovering_files {
            let overlay = ui.painter_at(rect);
            overlay.rect_filled(
                rect,
                egui::CornerRadius::ZERO,
                theme::VOID.gamma_multiply(0.86),
            );
            let drop = rect.shrink(48.0);
            dashed_rect(
                &overlay,
                drop,
                theme::LINE2,
                8.0,
                7.0,
                Stroke::new(1.5, theme::BLUE),
            );
            ptext(
                &overlay,
                rect.center() - Vec2::new(0.0, 12.0),
                Align2::CENTER_CENTER,
                "RELEASE TO INSPECT",
                15.0,
                theme::narrow_semi(),
                theme::BONE_HI,
                3.0,
            );
            ptext(
                &overlay,
                rect.center() + Vec2::new(0.0, 14.0),
                Align2::CENTER_CENTER,
                "STL / 3MF",
                9.0,
                theme::narrow(),
                theme::DIM,
                2.0,
            );
        }
    }

    fn camera_input(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        response: &egui::Response,
        aspect: f32,
    ) -> bool {
        let pointer = ui.ctx().input(|i| i.pointer.hover_pos());
        let inside = pointer.map(|p| rect.contains(p)).unwrap_or(false);
        let mut moved = false;
        if inside {
            if response.dragged_by(PointerButton::Primary) {
                self.camera.orbit(response.drag_delta());
                self.camera_flight = None;
                self.active_view = None;
                moved = true;
            }
            if response.dragged_by(PointerButton::Secondary)
                || response.dragged_by(PointerButton::Middle)
            {
                self.camera.pan(
                    response.drag_delta(),
                    Vec2::new(rect.width(), rect.height()),
                );
                self.camera_flight = None;
                self.active_view = None;
                moved = true;
            }
            let cursor_ndc = pointer.map(|p| {
                Vec2::new(
                    (p.x - rect.center().x) / (rect.width().max(1.0) * 0.5),
                    -(p.y - rect.center().y) / (rect.height().max(1.0) * 0.5),
                )
            });
            let scroll = ui.ctx().input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.05 {
                let factor = (-scroll * 0.0022).exp();
                self.camera.zoom(factor, cursor_ndc, aspect);
                self.camera_flight = None;
                self.active_view = None;
                moved = true;
            }
            let pinch = ui.ctx().input(|i| i.zoom_delta());
            if (pinch - 1.0).abs() > 0.001 {
                self.camera.zoom(1.0 / pinch, cursor_ndc, aspect);
                self.camera_flight = None;
                self.active_view = None;
                moved = true;
            }
            let icon = if response.dragged() {
                CursorIcon::Grabbing
            } else {
                CursorIcon::Grab
            };
            ui.ctx().set_cursor_icon(icon);
        }
        moved
    }

    fn live_viewport(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        response: &egui::Response,
        frame: &mut eframe::Frame,
    ) {
        let aspect = (rect.width() / rect.height().max(1.0)).max(0.01);
        let ppp = ui.ctx().pixels_per_point();
        let moved = self.camera_input(ui, rect, response, aspect);
        if moved && let Some(stream) = &mut self.stream {
            stream.follow = false;
        }
        if let Some(stream) = &self.stream {
            if stream.follow && !stream.bounds.is_empty() {
                let mut fit = self.camera;
                fit.fit(&stream.bounds);
                let blend = 0.14;
                self.camera.target = self.camera.target.lerp(fit.target, blend);
                self.camera.distance += (fit.distance - self.camera.distance) * blend;
            }
            if ui.ctx().input(|i| i.key_pressed(Key::F)) {
                if !stream.bounds.is_empty() {
                    self.camera.fit(&stream.bounds);
                    self.active_view = None;
                }
                if let Some(stream) = &mut self.stream {
                    stream.follow = false;
                }
            }
        }

        let size = [
            ((rect.width() * ppp).round().max(1.0)) as u32,
            ((rect.height() * ppp).round().max(1.0)) as u32,
        ];
        if self.live_gpu.is_none()
            && let Some(render_state) = frame.wgpu_render_state()
            && let Some(stream) = &self.stream
        {
            self.live_gpu = LiveRenderer::new(render_state, stream.total.saturating_mul(3));
        }
        let mut texture = None;
        let (Some(stream), Some(gpu)) = (&self.stream, &mut self.live_gpu) else {
            self.loading_state(ui, rect, "", Instant::now());
            return;
        };
        if self.live_uploaded < stream.positions.len()
            && gpu.append(&stream.positions[self.live_uploaded..])
        {
            self.live_uploaded = stream.positions.len();
        }
        if let Some(render_state) = frame.wgpu_render_state() {
            texture = gpu.render(render_state, &self.camera, size, aspect);
        }

        let painter = ui.painter_at(rect);
        draw_plate(&painter, rect);
        match texture {
            Some(texture) => {
                painter.image(
                    texture,
                    rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            None => {
                ptext(
                    &painter,
                    rect.center(),
                    Align2::CENTER_CENTER,
                    "WAITING FOR THE FIRST TRIANGLES",
                    10.0,
                    theme::narrow_semi(),
                    theme::DIM,
                    1.8,
                );
            }
        }

        let mut x = rect.left() + 26.0;
        let y = rect.top() + 28.0;
        let mut chip = |text: String, color: Color32| {
            let galley =
                painter.layout_job(theme::text_job(text, 9.0, theme::narrow_semi(), color, 1.5));
            let chip = Rect::from_min_size(Pos2::new(x, y), galley.size() + Vec2::new(18.0, 12.0));
            painter.rect_filled(chip, egui::CornerRadius::ZERO, theme::PANEL);
            painter.rect_stroke(
                chip,
                egui::CornerRadius::ZERO,
                Stroke::new(1.0, theme::LINE2),
                StrokeKind::Inside,
            );
            painter.galley(chip.min + Vec2::new(9.0, 6.0), galley, color);
            x = chip.right() + 8.0;
        };
        chip("STL".to_string(), theme::BONE);
        chip("LARGE MODEL".to_string(), theme::BONE);
        chip(
            if stream.verifying {
                "VERIFYING".to_string()
            } else {
                "STREAMING".to_string()
            },
            theme::BLUE,
        );
        chip(
            format!(
                "{} / {} TRIANGLES",
                theme::fmt_int(stream.scan.triangles),
                theme::fmt_int(stream.total)
            ),
            theme::DIM,
        );

        self.draw_scan_filmstrip(&painter, rect);
        ui.ctx().request_repaint();
    }

    fn draw_scan_filmstrip(&self, painter: &egui::Painter, rect: Rect) {
        let Some(stream) = &self.stream else {
            return;
        };
        let strip = Rect::from_min_max(
            Pos2::new(rect.left() + 26.0, rect.bottom() - 100.0),
            Pos2::new(rect.right() - 26.0, rect.bottom() - 46.0),
        );
        let labels = if strip.width() >= 720.0 {
            [
                "OPEN EDGE",
                "NON-MANIFOLD",
                "WINDING",
                "VOLUME",
                "SHELLS",
                "DEGENERATE",
            ]
        } else {
            [
                "OPEN EDGE",
                "NON-MANIF",
                "WINDING",
                "VOLUME",
                "SHELLS",
                "DEGEN",
            ]
        };
        let colors = [
            theme::kind_color(DefectKind::OpenEdge),
            theme::kind_color(DefectKind::NonManifold),
            theme::kind_color(DefectKind::InconsistentWinding),
            theme::BONE,
            theme::BONE,
            theme::kind_color(DefectKind::DegenerateFace),
        ];
        let fraction = stream.scan.fraction();
        let header_y = strip.top() - 18.0;
        ptext(
            painter,
            Pos2::new(strip.left(), header_y),
            Align2::LEFT_CENTER,
            "ANALYSIS FILMSTRIP / LIVE",
            8.0,
            theme::narrow_semi(),
            theme::DIM,
            1.8,
        );
        ptext(
            painter,
            Pos2::new(strip.right(), header_y),
            Align2::RIGHT_CENTER,
            format!(
                "{} / {} TRIANGLES · {:.0}%",
                theme::fmt_int(stream.scan.triangles),
                theme::fmt_int(stream.total),
                fraction * 100.0
            ),
            8.0,
            theme::narrow(),
            theme::DIM,
            0.5,
        );
        painter.rect_filled(strip, egui::CornerRadius::ZERO, theme::PANEL);
        painter.rect_stroke(
            strip,
            egui::CornerRadius::ZERO,
            Stroke::new(1.0, theme::LINE2),
            StrokeKind::Inside,
        );
        let line_x = if stream.verifying {
            strip.left() + (stream.started.elapsed().as_secs_f32() * 0.32).fract() * strip.width()
        } else {
            strip.left() + fraction * strip.width()
        };
        painter.vline(line_x, strip.y_range(), Stroke::new(2.0, theme::BLUE));
        painter.hline(
            strip.left()..=strip.left() + strip.width() * fraction,
            strip.bottom() - 1.0,
            Stroke::new(1.0, theme::BLUE),
        );
        let cell_width = (strip.width() - 5.0) / 6.0;
        let values = [
            theme::fmt_int(self.scan_display[0].round() as usize),
            theme::fmt_int(self.scan_display[1].round() as usize),
            theme::fmt_int(self.scan_display[2].round() as usize),
            theme::fmt_volume(self.scan_display[3] as f64),
            theme::fmt_int(self.scan_display[4].round() as usize),
            theme::fmt_int(self.scan_display[5].round() as usize),
        ];
        for index in 0..6 {
            let left = strip.left() + index as f32 * (cell_width + 1.0);
            let cell = Rect::from_min_max(
                Pos2::new(left, strip.top()),
                Pos2::new(left + cell_width, strip.bottom()),
            );
            if index > 0 {
                painter.vline(left, strip.y_range(), Stroke::new(1.0, theme::LINE));
            }
            ptext(
                painter,
                Pos2::new(cell.left() + 9.0, cell.top() + 7.0),
                Align2::LEFT_TOP,
                labels[index],
                7.0,
                theme::narrow_semi(),
                theme::DIM,
                1.0,
            );
            let value_color = if matches!(index, 0 | 1 | 2 | 5) && self.scan_display[index] > 0.5 {
                colors[index]
            } else {
                theme::BONE
            };
            ptext(
                painter,
                Pos2::new(cell.left() + 9.0, cell.top() + 24.0),
                Align2::LEFT_TOP,
                values[index].clone(),
                11.0,
                theme::narrow_semi(),
                value_color,
                0.5,
            );
        }
    }

    fn ready_viewport(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        response: &egui::Response,
        frame: &mut eframe::Frame,
    ) {
        let aspect = (rect.width() / rect.height().max(1.0)).max(0.01);
        let ppp = ui.ctx().pixels_per_point();
        let _ = self.camera_input(ui, rect, response, aspect);

        if self.gpu_defects_dirty {
            self.rebuild_defects();
        }

        let size = [
            ((rect.width() * ppp).round().max(1.0)) as u32,
            ((rect.height() * ppp).round().max(1.0)) as u32,
        ];

        if self.gpu.is_none()
            && let (Some(render_state), Some(scene)) =
                (frame.wgpu_render_state(), self.scene.as_ref())
        {
            let started = Instant::now();
            let info = render_state.adapter.get_info();
            self.gpu_name = format!(
                "{} · {}",
                format!("{:?}", info.backend).to_uppercase(),
                truncate(&info.name.to_uppercase(), 24)
            );
            let mut gpu = SceneRenderer::new(
                render_state,
                scene,
                &self.reports,
                &self.visible,
                &self.filters,
            );
            gpu.set_selection(if self.theater.is_some() || self.orientation_active() {
                None
            } else {
                self.selection
            });
            self.gpu = Some(gpu);
            self.upload_ms = started.elapsed().as_secs_f64() * 1000.0;
        }

        let mut texture = None;
        let orientation = self.orientation_active();
        let focus = if orientation {
            None
        } else {
            self.focus_params()
        };
        let theater_selection = if self.theater.is_some() || orientation {
            None
        } else {
            self.selection
        };
        let grid = self.show_grid && self.theater.is_none() && !orientation;
        let defects = !orientation;
        let model = if orientation {
            self.orientation_matrix()
        } else {
            Mat4::IDENTITY
        };
        if let Some(render_state) = frame.wgpu_render_state()
            && let Some(gpu) = &mut self.gpu
        {
            gpu.set_selection(theater_selection);
            texture = gpu.render(
                render_state,
                &self.camera,
                size,
                aspect,
                &self.visible,
                grid,
                orientation,
                defects,
                model,
                focus,
            );
        }

        let painter = ui.painter_at(rect);
        match texture {
            Some(texture) => {
                painter.image(
                    texture,
                    rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            None => {
                ptext(
                    &painter,
                    rect.center(),
                    Align2::CENTER_CENTER,
                    "INITIALISING GPU",
                    10.0,
                    theme::narrow_semi(),
                    theme::DIM,
                    3.0,
                );
            }
        }

        self.draw_scene_overlays(ui, rect, aspect);
    }

    fn draw_scene_overlays(&mut self, ui: &mut Ui, rect: Rect, aspect: f32) {
        let painter = ui.painter_at(rect);
        let frame = rect.shrink(18.0);
        painter.rect_stroke(
            frame,
            egui::CornerRadius::ZERO,
            Stroke::new(1.0, theme::LINE2.gamma_multiply(0.9)),
            StrokeKind::Inside,
        );
        theme::corner_ticks(&painter, frame, theme::BONE.gamma_multiply(0.28), 12.0);

        let Some(scene) = &self.scene else {
            return;
        };
        let orientation = self.orientation_active();

        let mut x = rect.left() + 26.0;
        let y = rect.top() + 24.0;
        let total_defects = self.summary.open_edges
            + self.summary.non_manifold_edges
            + self.summary.non_manifold_vertices
            + self.summary.inconsistent_edges
            + self.summary.degenerate_triangles;
        let mut tags: Vec<(String, Color32)> =
            vec![(scene.format.label().to_string(), theme::BONE)];
        if scene.format == Format::ThreeMf && !orientation {
            if rect.width() >= 760.0 {
                tags.push((format!("{} OBJECTS", scene.objects.len()), theme::BONE));
                if !scene.plates.is_empty() {
                    tags.push((format!("{} PLATES", scene.plates.len()), theme::BONE));
                }
            } else if scene.plates.is_empty() {
                tags.push((format!("{} OBJ", scene.objects.len()), theme::BONE));
            } else {
                tags.push((
                    format!(
                        "{} OBJ / {} PLATES",
                        scene.objects.len(),
                        scene.plates.len()
                    ),
                    theme::BONE,
                ));
            }
        }
        if orientation {
            tags.push(("ORIENTATION".to_string(), theme::BONE));
            if rect.width() >= 900.0
                && let Some(orientation) = &self.orientation
                && let Some(index) = orientation.selected
                && let Some(candidate) = orientation.report.candidates.get(index)
            {
                tags.push((truncate(&candidate.name, 24), theme::BONE));
            }
            tags.push(("BED 256 MM".to_string(), theme::DIM));
            if scene.format == Format::Stl && scene.assumed_mm {
                tags.push(("MM ASSUMED".to_string(), theme::DIM));
            }
        } else {
            if self.recovered {
                tags.push(("RECOVERED".to_string(), theme::BONE));
            } else if self
                .repair
                .as_ref()
                .is_some_and(|session| session.preview_active)
            {
                tags.push(("REPAIRED PREVIEW".to_string(), theme::BONE));
            } else if self.repaired {
                tags.push(("REPAIRED".to_string(), theme::BONE));
            }
            if self.theater.is_some() {
                tags.push(("THEATER".to_string(), theme::BONE));
            } else {
                tags.push((
                    if self.show_grid {
                        "GRID ON"
                    } else {
                        "GRID OFF"
                    }
                    .to_string(),
                    if self.show_grid {
                        theme::BONE
                    } else {
                        theme::DIM
                    },
                ));
            }
            if scene.format == Format::Stl && scene.assumed_mm {
                tags.push(("MM ASSUMED".to_string(), theme::DIM));
            }
            if !self.sidebar_open {
                tags.push(("INSPECTOR HIDDEN · I".to_string(), theme::BLUE));
            }
            let tag_verdict = self.verdict();
            tags.push((
                if tag_verdict == Verdict::Watertight {
                    "CLEAN".to_string()
                } else if total_defects == 0 {
                    "REVIEW".to_string()
                } else {
                    format!("{} DEFECTS", theme::fmt_int(total_defects))
                },
                match tag_verdict {
                    Verdict::Watertight => theme::BONE,
                    Verdict::Warning => theme::AMBER,
                    Verdict::Failed => theme::RED_TEXT,
                },
            ));
        }
        for (text, color) in tags {
            let galley =
                painter.layout_job(theme::text_job(text, 9.0, theme::narrow_semi(), color, 1.5));
            let chip = Rect::from_min_size(Pos2::new(x, y), galley.size() + Vec2::new(18.0, 12.0));
            painter.rect_filled(chip, egui::CornerRadius::ZERO, theme::PANEL);
            painter.rect_stroke(
                chip,
                egui::CornerRadius::ZERO,
                Stroke::new(1.0, theme::LINE2),
                StrokeKind::Inside,
            );
            painter.galley(chip.min + Vec2::new(9.0, 6.0), galley, color);
            x = chip.right() + 8.0;
        }

        if self.theater.is_none()
            && !orientation
            && let Some(index) = self.selection
            && let Some(object) = scene.objects.get(index)
            && self.visible.get(index).copied().unwrap_or(true)
        {
            draw_box_edges(
                &painter,
                &self.camera,
                &object.mesh.bounds(),
                rect,
                aspect,
                theme::BLUE.gamma_multiply(0.55),
            );
        }

        draw_axis_gizmo(&painter, &self.camera, rect, aspect);

        let units_per_px =
            2.0 * self.camera.distance * (self.camera.fov_y * 0.5).tan() / rect.height().max(1.0);
        let wide_overlay = rect.width() >= 820.0;
        if let Some((mm, px)) = nice_scale(units_per_px) {
            let bar = Rect::from_min_max(
                Pos2::new(rect.right() - 26.0 - px, rect.top() + 118.0),
                Pos2::new(rect.right() - 26.0, rect.top() + 118.0),
            );
            painter.hline(bar.x_range(), bar.center().y, Stroke::new(1.0, theme::DIM));
            painter.vline(
                bar.left(),
                bar.center().y - 4.0..=bar.center().y + 4.0,
                Stroke::new(1.0, theme::DIM),
            );
            painter.vline(
                bar.right(),
                bar.center().y - 4.0..=bar.center().y + 4.0,
                Stroke::new(1.0, theme::DIM),
            );
            ptext(
                &painter,
                Pos2::new(bar.center().x, bar.top() - 3.0),
                Align2::CENTER_BOTTOM,
                format!("{mm} MM"),
                8.0,
                theme::narrow_semi(),
                theme::DIM,
                1.5,
            );
        }

        if orientation {
            self.draw_orientation_bed(&painter, rect, aspect);
        } else {
            let counts = [
                (DefectKind::OpenEdge, self.summary.open_edges),
                (
                    DefectKind::NonManifold,
                    self.summary.non_manifold_edges + self.summary.non_manifold_vertices,
                ),
                (
                    DefectKind::InconsistentWinding,
                    self.summary.inconsistent_edges,
                ),
                (
                    DefectKind::DegenerateFace,
                    self.summary.degenerate_triangles,
                ),
            ];
            let short_legend = rect.width() < 840.0;
            let legend_width = if short_legend { 110.0 } else { 150.0 };
            let legend_step = if short_legend { 118.0 } else { 158.0 };
            let mut lx = rect.left() + 26.0;
            let ly = rect.bottom() - 52.0;
            for (slot, (kind, count)) in counts.into_iter().enumerate() {
                let color = theme::kind_color(kind);
                let enabled = self.filters[slot];
                let item =
                    Rect::from_min_size(Pos2::new(lx, ly - 13.0), Vec2::new(legend_width, 28.0));
                let response = ui.interact(item, Id::unique(("legend", slot)), Sense::click());
                let line_color = if enabled {
                    color
                } else {
                    color.gamma_multiply(0.28)
                };
                painter.hline(
                    item.left()..=item.left() + 18.0,
                    item.center().y,
                    Stroke::new(2.0, line_color),
                );
                ptext(
                    &painter,
                    Pos2::new(item.left() + 26.0, item.center().y),
                    Align2::LEFT_CENTER,
                    if short_legend {
                        match kind {
                            DefectKind::NonManifold => "NON-MANIF",
                            DefectKind::InconsistentWinding => "FLIPPED",
                            DefectKind::DegenerateFace => "DEGEN",
                            _ => "OPEN EDGE",
                        }
                        .to_string()
                    } else {
                        kind.label().to_uppercase()
                    },
                    8.0,
                    theme::narrow_semi(),
                    if enabled { theme::BONE } else { theme::FAINT },
                    1.1,
                );
                ptext(
                    &painter,
                    Pos2::new(item.right(), item.center().y),
                    Align2::RIGHT_CENTER,
                    if count == 0 {
                        "0".to_string()
                    } else {
                        theme::fmt_int(count)
                    },
                    8.0,
                    theme::narrow_med(),
                    if enabled && count > 0 {
                        color
                    } else {
                        theme::FAINT
                    },
                    0.8,
                );
                focus_ring(ui, &response);
                if activated(ui, &response) {
                    let next = !self.filters[slot];
                    self.filters[slot] = next;
                    if !next
                        && self
                            .theater
                            .is_some_and(|theater| theater.marker.kind == kind)
                    {
                        self.theater = None;
                        self.camera_flight = None;
                    }
                    self.gpu_defects_dirty = true;
                }
                if response.hovered() {
                    response.clone().on_hover_text("Toggle this defect layer");
                }
                lx += legend_step;
            }
        }

        if wide_overlay {
            ptext(
                &painter,
                Pos2::new(rect.right() - 26.0, rect.bottom() - 42.0),
                Align2::RIGHT_CENTER,
                "LMB ORBIT / RMB PAN / SCROLL ZOOM",
                8.0,
                theme::narrow(),
                theme::DIM,
                1.2,
            );
        }

        if !orientation {
            let markers = self.markers.clone();
            let theater = self.theater;
            let mut pending_focus: Option<(usize, DefectMarker)> = None;
            let mut placed: Vec<Pos2> = Vec::new();
            let mut number = 0;
            for (object_index, marker) in markers {
                if !self.visible.get(object_index).copied().unwrap_or(true) {
                    continue;
                }
                if !self.filters[crate::gpu::defect_slot(marker.kind)] {
                    continue;
                }
                if let Some(theater) = theater
                    && !(object_index == theater.object
                        && marker.kind == theater.marker.kind
                        && marker.position == theater.marker.position)
                {
                    continue;
                }
                let Some(point) =
                    self.camera
                        .project(Vec3::from_array(marker.position), rect, aspect)
                else {
                    continue;
                };
                if !rect.shrink(20.0).contains(point) {
                    continue;
                }
                if placed.iter().any(|other| other.distance(point) < 50.0) {
                    continue;
                }
                placed.push(point);
                number += 1;
                let color = theme::kind_color(marker.kind);
                let mut center = point + Vec2::new(22.0, -22.0);
                center.x = center.x.clamp(rect.left() + 30.0, rect.right() - 30.0);
                center.y = center.y.clamp(rect.top() + 60.0, rect.bottom() - 66.0);
                let circle = Rect::from_center_size(center, Vec2::splat(28.0));
                let response = ui.interact(
                    circle,
                    Id::unique(("callout", object_index, number)),
                    Sense::click(),
                );
                let dir = (center - point).normalized();
                painter.line_segment(
                    [point, center - dir * 14.0],
                    Stroke::new(1.0, color.gamma_multiply(0.7)),
                );
                painter.line_segment(
                    [point - Vec2::new(3.5, 0.0), point + Vec2::new(3.5, 0.0)],
                    Stroke::new(1.0, color),
                );
                painter.line_segment(
                    [point - Vec2::new(0.0, 3.5), point + Vec2::new(0.0, 3.5)],
                    Stroke::new(1.0, color),
                );
                painter.circle_filled(circle.center(), 14.0, theme::VOID);
                painter.circle_stroke(
                    circle.center(),
                    14.0,
                    Stroke::new(if response.hovered() { 1.5 } else { 1.0 }, color),
                );
                if theater.is_some() {
                    painter.circle_stroke(point, 18.0, Stroke::new(1.0, theme::BLUE));
                }
                ptext(
                    &painter,
                    circle.center(),
                    Align2::CENTER_CENTER,
                    number.to_string(),
                    9.0,
                    theme::narrow_semi(),
                    color,
                    0.0,
                );
                if response.has_focus() {
                    painter.circle_stroke(circle.center(), 16.0, Stroke::new(1.0, theme::BLUE));
                }
                if activated(ui, &response) {
                    pending_focus = Some((object_index, marker));
                }
                let object_name = scene
                    .objects
                    .get(object_index)
                    .map(|o| o.name.clone())
                    .unwrap_or_else(|| "Object".to_string());
                response
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .on_hover_ui(|ui| {
                        ui.label(
                            RichText::new(format!("{} · {}", marker.kind.label(), marker.count))
                                .font(FontId::new(14.0, theme::narrow_med()))
                                .color(color),
                        );
                        ui.label(
                            RichText::new(format!("{object_name} — click for defect theater"))
                                .font(FontId::new(13.5, egui::FontFamily::Proportional))
                                .color(theme::DIM),
                        );
                    });
            }

            if let Some((object, marker)) = pending_focus {
                self.focus_marker(object, &marker);
            }
        }
        if self.theater.is_some() {
            self.draw_theater_loupe(ui, rect, aspect);
        }
    }

    fn draw_orientation_bed(&self, painter: &egui::Painter, rect: Rect, _aspect: f32) {
        ptext(
            painter,
            Pos2::new(rect.left() + 26.0, rect.bottom() - 74.0),
            Align2::LEFT_CENTER,
            "BED 256 × 256 MM / ASSUMED",
            8.0,
            theme::narrow_semi(),
            theme::DIM,
            1.5,
        );
    }

    fn draw_theater_loupe(&mut self, ui: &mut Ui, rect: Rect, aspect: f32) {
        let Some(theater) = self.theater else {
            return;
        };
        let strength = self.theater_strength();
        if strength <= 0.001 {
            return;
        }
        let Some(scene) = &self.scene else {
            return;
        };
        let Some(report) = self.reports.get(theater.object) else {
            return;
        };
        let painter = ui.painter_at(rect);
        let kind = theater.marker.kind;
        let ink = theme::kind_color(kind);
        let fade = |color: Color32| color.gamma_multiply(strength);
        let width = if rect.width() >= 820.0 { 260.0 } else { 208.0 };
        let height = if rect.height() >= 520.0 { 184.0 } else { 154.0 };
        let lift = (1.0 - strength) * 10.0;
        let panel = Rect::from_min_size(
            Pos2::new(
                rect.right() - 26.0 - width,
                rect.bottom() - 86.0 - height + lift,
            ),
            Vec2::new(width, height),
        );
        painter.rect_filled(
            panel,
            egui::CornerRadius::ZERO,
            Color32::from_black_alpha((248.0 * strength) as u8),
        );
        painter.rect_stroke(
            panel,
            egui::CornerRadius::ZERO,
            Stroke::new(1.0, theme::LINE2.gamma_multiply(strength)),
            StrokeKind::Inside,
        );
        theme::corner_ticks(
            &painter,
            panel.shrink(0.5),
            fade(ink.gamma_multiply(0.72)),
            9.0,
        );

        let header_y = panel.top() + 9.0;
        ptext(
            &painter,
            Pos2::new(panel.left() + 12.0, header_y),
            Align2::LEFT_TOP,
            format!("LOUPE / {}", kind.label().to_uppercase()),
            8.0,
            theme::narrow_semi(),
            fade(theme::BONE),
            1.5,
        );
        ptext(
            &painter,
            Pos2::new(panel.right() - 12.0, header_y - 1.0),
            Align2::RIGHT_TOP,
            format!("× {}", theme::fmt_int(theater.marker.count as usize)),
            9.0,
            theme::narrow_semi(),
            fade(ink),
            0.8,
        );
        painter.hline(
            (panel.left() + 12.0)..=(panel.right() - 12.0),
            header_y + 14.0,
            Stroke::new(1.0, theme::LINE.gamma_multiply(strength)),
        );

        let map = Rect::from_min_max(
            Pos2::new(panel.left() + 12.0, header_y + 23.0),
            Pos2::new(panel.right() - 12.0, panel.bottom() - 30.0),
        );
        let center = Vec3::from_array(theater.marker.position);
        let radius = scene
            .objects
            .get(theater.object)
            .map(|object| object.mesh.bounds().radius())
            .unwrap_or_else(|| scene.bounds().radius())
            .max(1.0)
            * 0.22;
        let lines = local_defect_lines(report, &theater.marker, radius);
        let (right, up, _) = self.camera.basis();
        let project = |point: [f32; 3]| {
            let offset = Vec3::from_array(point) - center;
            Vec2::new(offset.dot(right), -offset.dot(up))
        };
        let map_center = map.center();
        painter.vline(
            map_center.x,
            (map_center.y - 7.0)..=(map_center.y + 7.0),
            Stroke::new(1.0, theme::LINE2.gamma_multiply(strength)),
        );
        painter.hline(
            (map_center.x - 7.0)..=(map_center.x + 7.0),
            map_center.y,
            Stroke::new(1.0, theme::LINE2.gamma_multiply(strength)),
        );

        if lines.is_empty() {
            if kind == DefectKind::DegenerateFace {
                let tri = [
                    map_center + Vec2::new(-28.0, 16.0),
                    map_center + Vec2::new(28.0, 16.0),
                    map_center + Vec2::new(0.0, -18.0),
                ];
                let stroke = Stroke::new(1.5, fade(ink));
                painter.line_segment([tri[0], tri[1]], stroke);
                painter.line_segment([tri[1], tri[2]], stroke);
                painter.line_segment([tri[2], tri[0]], stroke);
                for point in tri {
                    painter.circle_filled(point, 2.2, fade(ink));
                }
                painter.line_segment(
                    [
                        map_center - Vec2::new(10.0, 0.0),
                        map_center + Vec2::new(10.0, 0.0),
                    ],
                    Stroke::new(1.0, fade(theme::BONE)),
                );
                ptext(
                    &painter,
                    map_center + Vec2::new(0.0, 32.0),
                    Align2::CENTER_TOP,
                    "ZERO AREA",
                    7.5,
                    theme::narrow_semi(),
                    fade(theme::BONE),
                    1.2,
                );
            } else {
                ptext(
                    &painter,
                    map_center,
                    Align2::CENTER_CENTER,
                    "NO LOCAL EDGES",
                    8.0,
                    theme::narrow_semi(),
                    fade(theme::DIM),
                    1.0,
                );
            }
        } else {
            let mut min = Vec2::splat(f32::INFINITY);
            let mut max = Vec2::splat(f32::NEG_INFINITY);
            let projected: Vec<(Vec2, Vec2)> = lines
                .iter()
                .map(|line| {
                    let a = project(line.a);
                    let b = project(line.b);
                    min = min.min(a).min(b);
                    max = max.max(a).max(b);
                    (a, b)
                })
                .collect();
            let span = max - min;
            let fit = ((map.width() - 24.0) / span.x.max(1e-5))
                .min((map.height() - 24.0) / span.y.max(1e-5));
            let scale = fit.clamp(0.02, 80.0);
            let anchor = (min + max) * 0.5;
            let screen = |point: Vec2| map_center + (point - anchor) * scale;

            let stroke = Stroke::new(1.5, fade(ink));
            let mut vertices: HashMap<(i32, i32), (Pos2, usize)> = HashMap::new();
            for (a, b) in &projected {
                let a = screen(*a);
                let b = screen(*b);
                painter.line_segment([a, b], stroke);
                if kind == DefectKind::InconsistentWinding {
                    let dir = (b - a).normalized();
                    if dir.length_sq() > 0.1 {
                        let side = Vec2::new(-dir.y, dir.x);
                        let mid = a + (b - a) * 0.5;
                        painter.line_segment(
                            [mid, mid - dir * 4.5 + side * 2.2],
                            Stroke::new(1.0, fade(theme::BONE)),
                        );
                        painter.line_segment(
                            [mid, mid - dir * 4.5 - side * 2.2],
                            Stroke::new(1.0, fade(theme::BONE)),
                        );
                    }
                }
                if kind == DefectKind::NonManifold {
                    let mid = a + (b - a) * 0.5;
                    painter.circle_filled(mid, 1.8, fade(theme::BONE));
                }
                for point in [a, b] {
                    let key = (
                        (point.x * 1000.0).round() as i32,
                        (point.y * 1000.0).round() as i32,
                    );
                    vertices
                        .entry(key)
                        .and_modify(|(_, count)| *count += 1)
                        .or_insert((point, 1));
                }
            }
            for (point, valence) in vertices.values() {
                painter.circle_filled(*point, 2.1, fade(ink));
                if *valence != 2 {
                    let label = if *valence > 3 {
                        "3+".to_string()
                    } else {
                        valence.to_string()
                    };
                    ptext(
                        &painter,
                        *point + Vec2::new(3.0, -3.0),
                        Align2::LEFT_BOTTOM,
                        label,
                        7.0,
                        theme::narrow_semi(),
                        fade(if *valence == 1 { theme::BONE } else { ink }),
                        0.4,
                    );
                }
            }
        }

        let footer_y = panel.bottom() - 14.0;
        let detail = match kind {
            DefectKind::OpenEdge => "BOUNDARY",
            DefectKind::NonManifold => "3+ FACES",
            DefectKind::InconsistentWinding => "ARROWS",
            DefectKind::DegenerateFace => "ZERO AREA",
        };
        let left = if lines.is_empty() {
            if kind == DefectKind::DegenerateFace {
                format!(
                    "ZERO AREA / {}",
                    theme::fmt_int(theater.marker.count as usize)
                )
            } else {
                theme::fmt_int(theater.marker.count as usize)
            }
        } else if width >= 240.0 {
            format!(
                "{} · LOCAL {} / {}",
                detail,
                lines.len(),
                theme::fmt_int(theater.marker.count as usize)
            )
        } else {
            format!(
                "LOCAL {} / {}",
                lines.len(),
                theme::fmt_int(theater.marker.count as usize)
            )
        };
        ptext(
            &painter,
            Pos2::new(panel.left() + 12.0, footer_y),
            Align2::LEFT_CENTER,
            left,
            7.0,
            theme::narrow(),
            fade(theme::DIM),
            0.6,
        );
        ptext(
            &painter,
            Pos2::new(panel.right() - 12.0, footer_y),
            Align2::RIGHT_CENTER,
            if width >= 240.0 {
                "ESC EXIT · D NEXT"
            } else {
                "ESC EXIT"
            },
            7.0,
            theme::narrow_semi(),
            fade(theme::DIM),
            0.9,
        );

        let mut frame_points = Vec::new();
        for line in &lines {
            if let Some(point) = self.camera.project(Vec3::from_array(line.a), rect, aspect) {
                frame_points.push(point);
            }
            if let Some(point) = self.camera.project(Vec3::from_array(line.b), rect, aspect) {
                frame_points.push(point);
            }
        }
        let frame = if let (Some(min), Some(max)) = (
            frame_points
                .iter()
                .copied()
                .reduce(|a, b| Pos2::new(a.x.min(b.x), a.y.min(b.y))),
            frame_points
                .iter()
                .copied()
                .reduce(|a, b| Pos2::new(a.x.max(b.x), a.y.max(b.y))),
        ) {
            Rect::from_min_max(min, max).expand(18.0)
        } else if let Some(point) = self.camera.project(center, rect, aspect) {
            Rect::from_center_size(point, Vec2::splat(44.0))
        } else {
            Rect::NOTHING
        };
        if frame.is_positive() {
            let frame = frame.intersect(rect.shrink(22.0));
            painter.rect_stroke(
                frame,
                egui::CornerRadius::ZERO,
                Stroke::new(1.0, fade(theme::BONE.gamma_multiply(0.62))),
                StrokeKind::Inside,
            );
            theme::corner_ticks(
                &painter,
                frame.shrink(0.5),
                fade(theme::BONE.gamma_multiply(0.82)),
                8.0,
            );
            let corners = [
                frame.left_top(),
                frame.right_top(),
                frame.left_bottom(),
                frame.right_bottom(),
            ];
            let from = corners
                .into_iter()
                .min_by(|a, b| {
                    a.distance(panel.center())
                        .total_cmp(&b.distance(panel.center()))
                })
                .unwrap_or(frame.center());
            let target = if from.x < panel.center().x {
                panel.left_top()
            } else {
                panel.right_top()
            };
            painter.line_segment(
                [from, target],
                Stroke::new(1.0, fade(theme::BONE.gamma_multiply(0.22))),
            );
        }
    }

    fn empty_state(&mut self, ui: &mut Ui, rect: Rect) {
        let painter = ui.painter_at(rect);
        draw_plate(&painter, rect);
        let wide = rect.width() >= 900.0;
        let left_width = if wide { 480.0 } else { rect.width() - 80.0 };
        let left = Rect::from_min_size(
            rect.left_top() + Vec2::new(52.0, 72.0),
            Vec2::new(left_width.max(340.0), rect.height() - 144.0),
        );
        ui.scope_builder(
            UiBuilder::new()
                .max_rect(left)
                .layout(Layout::top_down(Align::Min)),
            |ui| {
                ui.set_max_width(left_width);
                theme::label(
                    ui,
                    "MODELVIEW",
                    73.0,
                    theme::narrow_semi(),
                    theme::BONE_HI,
                    2.2,
                );
                ui.add_space(10.0);
                let (line, _) = ui.allocate_exact_size(Vec2::new(120.0, 1.0), Sense::hover());
                ui.painter().hline(
                    line.x_range(),
                    line.center().y,
                    Stroke::new(1.0, theme::BLUE),
                );
                ui.add_space(18.0);
                ui.label(
                    RichText::new("See the mesh before the slicer does.")
                        .font(FontId::new(27.0, theme::narrow_med()))
                        .color(theme::BONE_HI),
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new(
                        "Open an STL or 3MF to check watertightness, dimensions, volume, and every open edge — on the GPU, without loading a project.",
                    )
                    .font(FontId::new(15.5, egui::FontFamily::Proportional))
                    .color(theme::BONE),
                );
                ui.add_space(20.0);
                ui.horizontal(|ui| {
                    let open = Button::new(
                        RichText::new("OPEN FILE")
                            .font(FontId::new(13.0, theme::narrow_semi()))
                            .color(theme::BONE)
                            .extra_letter_spacing(1.6),
                    )
                    .fill(theme::BLUE)
                    .stroke(Stroke::NONE)
                    .min_size(Vec2::new(140.0, 42.0));
                    let open_response = ui.add(open);
                    focus_ring(ui, &open_response);
                    if open_response.clicked() {
                        self.open_dialog(ui.ctx());
                    }
                    let sample = Button::new(
                        RichText::new("LOAD SAMPLE")
                            .font(FontId::new(13.0, theme::narrow_semi()))
                            .color(theme::BONE)
                            .extra_letter_spacing(1.6),
                    )
                    .stroke(Stroke::new(1.0, theme::BLUE))
                    .min_size(Vec2::new(140.0, 42.0));
                    let sample_response = ui.add(sample);
                    focus_ring(ui, &sample_response);
                    if sample_response.clicked() {
                        self.load_sample(ui.ctx());
                    }
                });
                ui.add_space(16.0);
                theme::label(
                    ui,
                    "STL BINARY / ASCII · 3MF BUILD ITEMS, PLATES, METADATA",
                    8.0,
                    theme::narrow_semi(),
                    theme::DIM,
                    1.3,
                );
                ui.add_space(4.0);
                theme::label(
                    ui,
                    "FILES STAY ON THIS MACHINE",
                    8.0,
                    theme::narrow_semi(),
                    theme::DIM,
                    1.6,
                );
            },
        );

        if wide {
            let drop = Rect::from_min_size(
                Pos2::new(rect.right() - 68.0 - 300.0, rect.center().y - 105.0),
                Vec2::new(300.0, 210.0),
            );
            dashed_rect(
                &painter,
                drop,
                theme::BLUE,
                7.0,
                6.0,
                Stroke::new(1.0, theme::BLUE),
            );
            draw_cube_glyph(
                &painter,
                Pos2::new(drop.center().x, drop.top() + 70.0),
                theme::BONE.gamma_multiply(0.6),
            );
            ptext(
                &painter,
                Pos2::new(drop.center().x, drop.bottom() - 52.0),
                Align2::CENTER_CENTER,
                "DROP A MODEL",
                10.0,
                theme::narrow_semi(),
                theme::BONE,
                2.2,
            );
            ptext(
                &painter,
                Pos2::new(drop.center().x, drop.bottom() - 33.0),
                Align2::CENTER_CENTER,
                "STL / 3MF",
                8.5,
                theme::narrow(),
                theme::DIM,
                1.8,
            );
        }

        ptext(
            &painter,
            Pos2::new(rect.left() + 26.0, rect.bottom() - 34.0),
            Align2::LEFT_CENTER,
            format!(
                "LMB ORBIT / RMB PAN / SCROLL ZOOM / {}+1-7 VIEWS",
                mod_key()
            ),
            8.0,
            theme::narrow(),
            theme::DIM,
            1.2,
        );
    }

    fn loading_state(&self, ui: &mut Ui, rect: Rect, name: &str, started: Instant) {
        let painter = ui.painter_at(rect);
        draw_plate(&painter, rect);
        let center = rect.center();
        ptext(
            &painter,
            Pos2::new(center.x, center.y - 38.0),
            Align2::CENTER_CENTER,
            truncate(name, 44),
            28.0,
            theme::narrow_med(),
            theme::BONE_HI,
            0.4,
        );
        ptext(
            &painter,
            Pos2::new(center.x, center.y - 12.0),
            Align2::CENTER_CENTER,
            if self.recovery_load {
                "RESOLVING EXTERNAL PARTS / PRESERVING PLATES"
            } else {
                "PARSING THE FILE AND CHECKING TOPOLOGY"
            },
            8.5,
            theme::narrow_semi(),
            if self.recovery_load {
                theme::AMBER
            } else {
                theme::DIM
            },
            2.0,
        );
        let bar =
            Rect::from_center_size(Pos2::new(center.x, center.y + 34.0), Vec2::new(360.0, 2.0));
        painter.hline(bar.x_range(), bar.center().y, Stroke::new(1.0, theme::LINE));
        let progress = (started.elapsed().as_secs_f32() * 0.55).fract();
        let segment = 90.0;
        let x = bar.left() + (bar.width() - segment) * progress;
        painter.hline(
            x..=x + segment,
            bar.center().y,
            Stroke::new(2.0, theme::BLUE),
        );
    }

    fn error_state(&mut self, ui: &mut Ui, rect: Rect, title: &str) {
        let painter = ui.painter_at(rect);
        draw_plate(&painter, rect);
        let center = rect.center();
        let recovery = self.recovery_available;
        ptext(
            &painter,
            Pos2::new(center.x, center.y - 84.0),
            Align2::CENTER_CENTER,
            if recovery {
                "PACKAGE NEEDS RECOVERY"
            } else {
                title
            },
            27.0,
            theme::narrow_med(),
            if recovery { theme::AMBER } else { theme::RED },
            0.3,
        );
        let content = Rect::from_center_size(
            Pos2::new(center.x, center.y - 4.0),
            Vec2::new((rect.width() - 160.0).clamp(320.0, 620.0), 96.0),
        );
        ui.scope_builder(
            UiBuilder::new()
                .max_rect(content)
                .layout(Layout::top_down(Align::Center)),
            |ui| {
                ui.set_max_width(content.width());
                ui.label(
                    RichText::new(if recovery {
                        "This 3MF keeps its objects in external model parts. Fix on load to resolve them, preserve every plate, and open the model. The source is not modified."
                    } else {
                        "The file may be missing, unreadable, or not a supported STL or 3MF. Check the path, then choose another file or load the sample."
                    })
                    .font(FontId::new(14.5, egui::FontFamily::Proportional))
                    .color(theme::BONE),
                );
            },
        );
        let buttons =
            Rect::from_center_size(Pos2::new(center.x, center.y + 86.0), Vec2::new(320.0, 40.0));
        ui.scope_builder(
            UiBuilder::new()
                .max_rect(buttons)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| {
                ui.horizontal(|ui| {
                    if recovery {
                        let fix = Button::new(
                            RichText::new("FIX ON LOAD")
                                .font(FontId::new(13.0, theme::narrow_semi()))
                                .color(theme::BONE)
                                .extra_letter_spacing(1.5),
                        )
                        .fill(theme::BLUE)
                        .stroke(Stroke::NONE)
                        .min_size(Vec2::new(156.0, 40.0));
                        let fix_response = ui.add(fix);
                        focus_ring(ui, &fix_response);
                        if fix_response.clicked() {
                            self.start_recovery(ui.ctx());
                        }
                        let open = Button::new(
                            RichText::new("OPEN ANOTHER")
                                .font(FontId::new(13.0, theme::narrow_semi()))
                                .color(theme::BONE)
                                .extra_letter_spacing(1.5),
                        )
                        .stroke(Stroke::new(1.0, theme::BLUE))
                        .min_size(Vec2::new(146.0, 40.0));
                        let open_response = ui.add(open);
                        focus_ring(ui, &open_response);
                        if open_response.clicked() {
                            self.open_dialog(ui.ctx());
                        }
                    } else {
                        let open = Button::new(
                            RichText::new("OPEN ANOTHER")
                                .font(FontId::new(13.0, theme::narrow_semi()))
                                .color(theme::BONE)
                                .extra_letter_spacing(1.5),
                        )
                        .fill(theme::BLUE)
                        .stroke(Stroke::NONE)
                        .min_size(Vec2::new(156.0, 40.0));
                        let open_response = ui.add(open);
                        focus_ring(ui, &open_response);
                        if open_response.clicked() {
                            self.open_dialog(ui.ctx());
                        }
                        let sample = Button::new(
                            RichText::new("LOAD SAMPLE")
                                .font(FontId::new(13.0, theme::narrow_semi()))
                                .color(theme::BONE)
                                .extra_letter_spacing(1.5),
                        )
                        .stroke(Stroke::new(1.0, theme::BLUE))
                        .min_size(Vec2::new(146.0, 40.0));
                        let sample_response = ui.add(sample);
                        focus_ring(ui, &sample_response);
                        if sample_response.clicked() {
                            self.load_sample(ui.ctx());
                        }
                    }
                });
            },
        );
        if let Some(path) = &self.file_path {
            ptext(
                &painter,
                Pos2::new(center.x, center.y + 132.0),
                Align2::CENTER_CENTER,
                truncate(&path.display().to_string(), 72),
                8.5,
                theme::narrow(),
                theme::DIM,
                1.0,
            );
        }
    }

    fn help_window(&mut self, ctx: &egui::Context) {
        if !self.help_open {
            return;
        }
        let mut open = self.help_open;
        egui::Window::new("Controls")
            .id(Id::unique("controls_window"))
            .order(egui::Order::Foreground)
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .fade_in(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL)
                    .stroke(Stroke::new(1.0, theme::LINE2))
                    .inner_margin(Margin::same(28))
                    .corner_radius(egui::CornerRadius::ZERO),
            )
            .show(ctx, |ui| {
                ui.set_width(480.0);
                theme::label(
                    ui,
                    "CONTROLS / MOUSE + KEYBOARD",
                    9.0,
                    theme::narrow_semi(),
                    theme::BONE,
                    2.4,
                );
                ui.add_space(6.0);
                theme::label(
                    ui,
                    "Viewport navigation follows BambuStudio.",
                    11.0,
                    egui::FontFamily::Proportional,
                    theme::BONE,
                    0.0,
                );
                ui.add_space(8.0);
                let modifier = mod_key();
                let rows = [
                    ("ORBIT", "LEFT DRAG".to_string()),
                    ("PAN", "RIGHT OR MIDDLE DRAG".to_string()),
                    ("ZOOM TO CURSOR", "SCROLL / PINCH".to_string()),
                    ("FRAME MODEL", "F".to_string()),
                    (
                        "HOME / TOP / FRONT / RIGHT",
                        format!("{modifier} + 1 / 2 / 4 / 7"),
                    ),
                    ("BOTTOM / BACK / LEFT", format!("{modifier} + 3 / 5 / 6")),
                    ("GRID", "G".to_string()),
                    ("NEXT OBJECT", "UP / DOWN".to_string()),
                    ("FRAME SELECTED", "ENTER".to_string()),
                    ("DEFECT THEATER / NEXT", "D".to_string()),
                    ("SHOW / HIDE SELECTED", "SPACE".to_string()),
                    ("INSPECTOR", "I".to_string()),
                    ("ORIENTATION", "O".to_string()),
                    ("THEATER / CLOSE / REVERT / CLEAR", "ESC".to_string()),
                    ("OPEN FILE", format!("{modifier} + O")),
                    ("SETTINGS", format!("{modifier} + ,")),
                    ("SEND TO SLICER", format!("{modifier} + ENTER")),
                    ("REPAIR PREVIEW", "R".to_string()),
                ];
                let row_height = if ctx.content_rect().height() < 760.0 {
                    20.0
                } else {
                    22.0
                };
                for (action, keys) in rows {
                    let width = ui.available_width();
                    let (row, _) =
                        ui.allocate_exact_size(Vec2::new(width, row_height), Sense::hover());
                    ptext(
                        ui.painter(),
                        Pos2::new(row.left(), row.center().y),
                        Align2::LEFT_CENTER,
                        action,
                        9.0,
                        theme::narrow_semi(),
                        theme::DIM,
                        1.5,
                    );
                    ptext(
                        ui.painter(),
                        Pos2::new(row.right(), row.center().y),
                        Align2::RIGHT_CENTER,
                        keys,
                        9.5,
                        theme::narrow_med(),
                        theme::BONE,
                        1.0,
                    );
                    ui.painter().hline(
                        row.x_range(),
                        row.bottom() - 0.5,
                        Stroke::new(1.0, theme::LINE),
                    );
                }
                ui.add_space(8.0);
                theme::label(
                    ui,
                    "STL carries no units; ModelView assumes millimetres.",
                    8.5,
                    theme::narrow(),
                    theme::DIM,
                    0.5,
                );
                ui.add_space(10.0);
                let close = Button::new(
                    RichText::new("CLOSE")
                        .font(FontId::new(12.0, theme::narrow_semi()))
                        .color(theme::BONE)
                        .extra_letter_spacing(1.6),
                )
                .stroke(Stroke::new(1.0, theme::BLUE))
                .min_size(Vec2::new(104.0, 36.0));
                let close_response = ui.add(close);
                focus_ring(ui, &close_response);
                if close_response.clicked() {
                    open = false;
                }
            });
        self.help_open = open;
    }

    fn maybe_screenshot(&mut self, ctx: &egui::Context) {
        let Some(config) = &self.screenshot else {
            return;
        };
        ctx.request_repaint();
        if self.screenshot_sent {
            return;
        }
        if self.screenshot_sent || self.frames < config.delay_frames {
            return;
        }
        if matches!(self.phase, Phase::Loading { .. })
            && !(config.allow_loading && self.stream.is_some())
        {
            return;
        }
        if self.repair_job.is_some() && self.frames < config.delay_frames.saturating_add(600) {
            return;
        }
        if (self.camera_flight.is_some()
            || self.orientation_job.is_some()
            || self
                .orientation_pose
                .is_some_and(|pose| pose.started.elapsed().as_secs_f32() < 0.55)
            || self
                .theater
                .as_ref()
                .is_some_and(|theater| theater.started.elapsed().as_secs_f32() < 0.40))
            && self.frames < config.delay_frames.saturating_add(600)
        {
            return;
        }
        let path = config.path.clone();
        self.screenshot_sent = true;
        let ctx = ctx.clone();
        let close_ctx = ctx.clone();
        let source = self.display_name.clone();
        let source_path = self
            .file_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "Built-in sample".to_string());
        ctx.request_screenshot(move |image| {
            save_screenshot(&path, &image, &source, &source_path);
            close_ctx.send_viewport_cmd(ViewportCommand::Close);
        });
    }
}

impl eframe::App for ModelView {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.frames += 1;
        self.step_theater(ui.ctx());
        self.step_stream(ui.ctx());
        self.step_orientation(ui.ctx());
        self.poll(ui.ctx());
        self.handle_shortcuts(ui.ctx());
        self.handle_drops(ui.ctx());

        egui::Panel::top("top_bar")
            .exact_size(66.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(Margin::same(0)),
            )
            .show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("status_bar")
            .exact_size(38.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::VOID)
                    .inner_margin(Margin::same(0)),
            )
            .show(ui, |ui| self.status_bar(ui));
        egui::Panel::left("index_rail")
            .exact_size(76.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(Margin::same(0)),
            )
            .show(ui, |ui| self.rail(ui));
        if self.sidebar_open {
            egui::Panel::right("inspector")
                .exact_size(440.0)
                .frame(
                    egui::Frame::new()
                        .fill(theme::PANEL)
                        .inner_margin(Margin::same(0)),
                )
                .show(ui, |ui| self.side_panel(ui));
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::VOID)
                    .inner_margin(Margin::same(0)),
            )
            .show(ui, |ui| self.viewport(ui, frame));

        self.sheet_backdrop(ui.ctx());
        self.repair_window(ui.ctx());
        self.settings_window(ui.ctx());
        self.help_window(ui.ctx());
        if self.notice.is_some() {
            ui.ctx().request_repaint_after(Duration::from_secs(1));
        }
        self.maybe_screenshot(ui.ctx());
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.settings.last_open_dir = self.last_dir.clone();
        eframe::set_value(storage, eframe::APP_KEY, &self.settings);
    }
}

fn draw_focus_ring(ui: &egui::Ui, rect: Rect) {
    ui.painter().rect_stroke(
        rect.expand(1.0),
        egui::CornerRadius::ZERO,
        Stroke::new(1.0, theme::BLUE),
        StrokeKind::Inside,
    );
}

fn local_defect_lines(report: &MeshReport, marker: &DefectMarker, radius: f32) -> Vec<DefectLine> {
    let center = Vec3::from_array(marker.position);
    let mut local: Vec<(f32, DefectLine)> = report
        .defect_lines
        .iter()
        .filter(|line| line.kind == marker.kind)
        .filter_map(|line| {
            let mid = (Vec3::from_array(line.a) + Vec3::from_array(line.b)) * 0.5;
            let distance = mid.distance(center);
            (distance <= radius).then_some((distance, *line))
        })
        .collect();
    if local.is_empty() {
        local = report
            .defect_lines
            .iter()
            .filter(|line| line.kind == marker.kind)
            .map(|line| {
                let mid = (Vec3::from_array(line.a) + Vec3::from_array(line.b)) * 0.5;
                (mid.distance(center), *line)
            })
            .collect();
    }
    local.sort_by(|a, b| a.0.total_cmp(&b.0));
    local.truncate(80);
    local.into_iter().map(|(_, line)| line).collect()
}

fn markers_from_reports(reports: &[MeshReport]) -> Vec<(usize, DefectMarker)> {
    reports
        .iter()
        .enumerate()
        .flat_map(|(index, report)| report.markers.iter().map(move |marker| (index, *marker)))
        .collect()
}

fn sheet_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(theme::PANEL)
        .stroke(Stroke::new(1.0, theme::LINE2))
        .inner_margin(Margin::same(28))
        .corner_radius(egui::CornerRadius::ZERO)
}

fn sheet_button(ui: &mut Ui, label: &str, width: f32, filled: bool) -> bool {
    let text = RichText::new(label)
        .font(FontId::new(12.5, theme::narrow_semi()))
        .color(theme::BONE)
        .extra_letter_spacing(1.4);
    let mut button = egui::Button::new(text)
        .min_size(Vec2::new(width, 36.0))
        .fill(if filled {
            theme::BLUE
        } else {
            Color32::TRANSPARENT
        });
    if !filled {
        button = button.stroke(Stroke::new(1.0, theme::BLUE));
    } else {
        button = button.stroke(Stroke::NONE);
    }
    let response = ui.add(button);
    focus_ring(ui, &response);
    response.clicked()
}

fn sheet_button_enabled(ui: &mut Ui, label: &str, width: f32, filled: bool, enabled: bool) -> bool {
    if enabled {
        return sheet_button(ui, label, width, filled);
    }
    let text = RichText::new(label)
        .font(FontId::new(12.5, theme::narrow_semi()))
        .color(theme::FAINT)
        .extra_letter_spacing(1.4);
    let button = egui::Button::new(text)
        .min_size(Vec2::new(width, 36.0))
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::new(1.0, theme::LINE2));
    let _ = ui.add_enabled(false, button);
    false
}

fn slicer_row(ui: &mut Ui, candidate: &SlicerCandidate, selected: Option<&SlicerChoice>) -> bool {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 56.0), Sense::click());
    let is_selected = selected
        .map(|choice| choice.kind == candidate.kind && choice.path == candidate.path)
        .unwrap_or(false);
    let painter = ui.painter();
    if is_selected {
        painter.rect_filled(rect, egui::CornerRadius::ZERO, theme::BLUE_DIM);
    } else if response.hovered() {
        painter.rect_filled(
            rect,
            egui::CornerRadius::ZERO,
            theme::ASH.gamma_multiply(0.7),
        );
    }
    painter.rect_stroke(
        rect,
        egui::CornerRadius::ZERO,
        Stroke::new(
            1.0,
            if is_selected {
                theme::BLUE
            } else {
                theme::LINE
            },
        ),
        StrokeKind::Inside,
    );
    let square = Rect::from_center_size(
        Pos2::new(rect.left() + 18.0, rect.center().y),
        Vec2::splat(16.0),
    );
    painter.rect_stroke(
        square,
        egui::CornerRadius::ZERO,
        Stroke::new(
            1.0,
            if is_selected {
                theme::BLUE
            } else {
                theme::LINE2
            },
        ),
        StrokeKind::Inside,
    );
    if is_selected {
        painter.rect_filled(square.shrink(4.0), egui::CornerRadius::ZERO, theme::BLUE);
    }
    ptext(
        painter,
        Pos2::new(rect.left() + 38.0, rect.top() + 10.0),
        Align2::LEFT_TOP,
        truncate(&candidate.name, 30),
        15.0,
        theme::narrow_med(),
        theme::BONE,
        0.2,
    );
    ptext(
        painter,
        Pos2::new(rect.left() + 38.0, rect.top() + 31.0),
        Align2::LEFT_TOP,
        truncate(&candidate.path.display().to_string(), 62),
        11.0,
        theme::narrow(),
        theme::DIM,
        0.2,
    );
    let present = candidate.path.exists();
    ptext(
        painter,
        Pos2::new(rect.right() - 12.0, rect.center().y),
        Align2::RIGHT_CENTER,
        if present { "FOUND" } else { "MISSING" },
        9.5,
        theme::narrow_semi(),
        if present { theme::DIM } else { theme::RED_TEXT },
        1.3,
    );
    focus_ring(ui, &response);
    if activated(ui, &response) {
        return true;
    }
    response.on_hover_cursor(CursorIcon::PointingHand);
    false
}

fn repair_option_row(
    ui: &mut Ui,
    label: &str,
    value: &mut bool,
    disabled: bool,
    locked: bool,
    count: String,
) -> bool {
    let width = ui.available_width();
    let interactive = !disabled && !locked;
    let sense = if interactive {
        Sense::click()
    } else {
        Sense::hover()
    };
    let compact = ui.ctx().content_rect().height() < 760.0;
    let height = if compact { 34.0 } else { 40.0 };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), sense);
    let painter = ui.painter();
    if interactive && response.hovered() {
        painter.rect_filled(
            rect,
            egui::CornerRadius::ZERO,
            theme::ASH.gamma_multiply(0.7),
        );
    }
    let square = Rect::from_center_size(
        Pos2::new(rect.left() + 10.0, rect.center().y),
        Vec2::splat(16.0),
    );
    let color = if disabled {
        theme::FAINT
    } else if locked {
        theme::DIM
    } else {
        theme::BONE
    };
    painter.rect_stroke(
        square,
        egui::CornerRadius::ZERO,
        Stroke::new(
            1.0,
            if disabled {
                theme::FAINT
            } else if *value {
                theme::BLUE
            } else {
                theme::LINE2
            },
        ),
        StrokeKind::Inside,
    );
    if *value {
        painter.rect_filled(
            square.shrink(4.0),
            egui::CornerRadius::ZERO,
            if disabled { theme::FAINT } else { theme::BLUE },
        );
    }
    ptext(
        painter,
        Pos2::new(rect.left() + 28.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        12.5,
        theme::narrow_med(),
        color,
        0.2,
    );
    ptext(
        painter,
        Pos2::new(rect.right() - 4.0, rect.center().y),
        Align2::RIGHT_CENTER,
        count,
        11.0,
        theme::narrow(),
        if disabled { theme::FAINT } else { theme::DIM },
        0.4,
    );
    if interactive {
        focus_ring(ui, &response);
        if activated(ui, &response) {
            *value = !*value;
            return true;
        }
        response.on_hover_cursor(CursorIcon::PointingHand);
    }
    false
}

fn repair_metrics(ui: &mut Ui, label: &str, summary: &MeshReport, color: Color32) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 42.0), Sense::hover());
    ptext(
        ui.painter(),
        Pos2::new(rect.left(), rect.top()),
        Align2::LEFT_TOP,
        label,
        9.5,
        theme::narrow_semi(),
        theme::DIM,
        1.8,
    );
    let text = format!(
        "OPEN {}   WINDING {}   DEGENERATE {}   NON-MANIFOLD {}   SHELLS {}",
        theme::fmt_int(summary.open_edges),
        theme::fmt_int(summary.inconsistent_edges),
        theme::fmt_int(summary.degenerate_triangles),
        theme::fmt_int(summary.non_manifold_edges + summary.non_manifold_vertices),
        theme::fmt_int(summary.shells),
    );
    ptext(
        ui.painter(),
        Pos2::new(rect.left(), rect.top() + 20.0),
        Align2::LEFT_TOP,
        text,
        12.0,
        theme::narrow_med(),
        color,
        0.2,
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OrientationAction {
    Analyse,
    Reset,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RepairAction {
    Keep,
    Revert,
    Save,
    Send,
}

fn focus_ring(ui: &egui::Ui, response: &egui::Response) {
    if response.has_focus() {
        draw_focus_ring(ui, response.rect);
    }
}

fn activated(ui: &egui::Ui, response: &egui::Response) -> bool {
    response.clicked()
        || (response.has_focus()
            && ui.input(|i| i.key_pressed(Key::Enter) || i.key_pressed(Key::Space)))
}

fn mod_key() -> &'static str {
    if cfg!(target_os = "macos") {
        "CMD"
    } else {
        "CTRL"
    }
}

#[allow(clippy::too_many_arguments)]
fn ptext(
    painter: &egui::Painter,
    pos: Pos2,
    anchor: Align2,
    text: impl Into<String>,
    size: f32,
    family: egui::FontFamily,
    color: Color32,
    tracking: f32,
) -> Rect {
    let galley = painter.layout_job(theme::text_job(text, size, family, color, tracking));
    let rect = anchor.anchor_size(pos, galley.size());
    painter.galley(rect.min, galley, color);
    rect
}

fn vsep(ui: &mut Ui, height: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(1.0, height), Sense::hover());
    ui.painter().vline(
        rect.center().x,
        rect.y_range(),
        Stroke::new(1.0, theme::LINE),
    );
    ui.add_space(10.0);
}

fn truncate(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    let keep = max_chars.saturating_sub(1);
    let mut out: String = text.chars().take(keep).collect();
    out.push('…');
    out
}

fn kv_row(ui: &mut Ui, width: f32, key: &str, value: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 32.0), Sense::hover());
    ptext(
        ui.painter(),
        Pos2::new(rect.left(), rect.center().y),
        Align2::LEFT_CENTER,
        key,
        8.0,
        theme::narrow_semi(),
        theme::DIM,
        1.6,
    );
    ptext(
        ui.painter(),
        Pos2::new(rect.right(), rect.center().y),
        Align2::RIGHT_CENTER,
        truncate(value, 34),
        9.5,
        theme::narrow_med(),
        theme::BONE,
        0.6,
    );
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0, theme::LINE),
    );
}

fn dashed_rect(
    painter: &egui::Painter,
    rect: Rect,
    _color: Color32,
    dash: f32,
    gap: f32,
    stroke: Stroke,
) {
    dashed_line(
        painter,
        rect.left_top(),
        rect.right_top(),
        dash,
        gap,
        stroke,
    );
    dashed_line(
        painter,
        rect.right_top(),
        rect.right_bottom(),
        dash,
        gap,
        stroke,
    );
    dashed_line(
        painter,
        rect.right_bottom(),
        rect.left_bottom(),
        dash,
        gap,
        stroke,
    );
    dashed_line(
        painter,
        rect.left_bottom(),
        rect.left_top(),
        dash,
        gap,
        stroke,
    );
}

fn dashed_line(painter: &egui::Painter, a: Pos2, b: Pos2, dash: f32, gap: f32, stroke: Stroke) {
    let vector = b - a;
    let length = vector.length();
    if length <= 0.0 {
        return;
    }
    let dir = vector / length;
    let mut t = 0.0;
    while t < length {
        let end = (t + dash).min(length);
        painter.line_segment([a + dir * t, a + dir * end], stroke);
        t = end + gap;
    }
}

fn draw_plate(painter: &egui::Painter, rect: Rect) {
    let frame = rect.shrink(18.0);
    painter.rect_stroke(
        frame,
        egui::CornerRadius::ZERO,
        Stroke::new(1.0, theme::LINE.gamma_multiply(0.8)),
        StrokeKind::Inside,
    );
    theme::corner_ticks(painter, frame, theme::BONE.gamma_multiply(0.22), 12.0);
}

fn draw_cube_glyph(painter: &egui::Painter, center: Pos2, color: Color32) {
    let w = 62.0;
    let h = 72.0;
    let d = 26.0;
    let stroke = Stroke::new(1.0, color);
    let front = Rect::from_center_size(center, Vec2::new(w, h));
    let offset = Vec2::new(d, -d * 0.62);
    let back = front.translate(offset);
    painter.rect_stroke(front, egui::CornerRadius::ZERO, stroke, StrokeKind::Inside);
    painter.rect_stroke(back, egui::CornerRadius::ZERO, stroke, StrokeKind::Inside);
    for (a, b) in [
        (front.left_top(), back.left_top()),
        (front.right_top(), back.right_top()),
        (front.left_bottom(), back.left_bottom()),
        (front.right_bottom(), back.right_bottom()),
    ] {
        painter.line_segment([a, b], stroke);
    }
}

fn draw_box_edges(
    painter: &egui::Painter,
    camera: &Camera,
    bounds: &Bounds,
    rect: Rect,
    aspect: f32,
    color: Color32,
) {
    if bounds.is_empty() {
        return;
    }
    let min = bounds.min;
    let max = bounds.max;
    let corners = [
        Vec3::new(min.x, min.y, min.z),
        Vec3::new(max.x, min.y, min.z),
        Vec3::new(max.x, max.y, min.z),
        Vec3::new(min.x, max.y, min.z),
        Vec3::new(min.x, min.y, max.z),
        Vec3::new(max.x, min.y, max.z),
        Vec3::new(max.x, max.y, max.z),
        Vec3::new(min.x, max.y, max.z),
    ];
    let projected: Option<Vec<Pos2>> = corners
        .iter()
        .map(|p| camera.project(*p, rect, aspect))
        .collect();
    let Some(points) = projected else {
        return;
    };
    let edges = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ];
    let stroke = Stroke::new(1.0, color);
    for (a, b) in edges {
        painter.line_segment([points[a], points[b]], stroke);
    }
}

fn draw_axis_gizmo(painter: &egui::Painter, camera: &Camera, rect: Rect, aspect: f32) {
    let origin = Pos2::new(rect.right() - 64.0, rect.top() + 64.0);
    let Some(base) = camera.project(camera.target, rect, aspect) else {
        return;
    };
    let axes = [
        (Vec3::X, "X", theme::BONE.gamma_multiply(0.55)),
        (Vec3::Y, "Y", theme::BONE.gamma_multiply(0.55)),
        (Vec3::Z, "Z", theme::BLUE),
    ];
    painter.circle_stroke(origin, 1.5, Stroke::new(1.0, theme::DIM));
    for (axis, label, color) in axes {
        let tip_world = camera.target + axis * (camera.distance * 0.12);
        let Some(tip) = camera.project(tip_world, rect, aspect) else {
            continue;
        };
        let delta = tip - base;
        if delta.length_sq() < 0.01 {
            continue;
        }
        let dir = delta.normalized();
        let end = origin + dir * 24.0;
        painter.line_segment([origin, end], Stroke::new(1.0, color));
        ptext(
            painter,
            origin + dir * 34.0,
            Align2::CENTER_CENTER,
            label,
            8.0,
            theme::narrow_semi(),
            color,
            0.0,
        );
    }
}

fn nice_scale(units_per_pixel: f32) -> Option<(String, f32)> {
    if !units_per_pixel.is_finite() || units_per_pixel <= 0.0 {
        return None;
    }
    let steps = [
        0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0,
    ];
    let mut best: Option<(f32, f32)> = None;
    for step in steps {
        let px = step / units_per_pixel;
        if (56.0..=150.0).contains(&px) {
            return Some((format_step(step), px));
        }
        let distance = (px - 100.0).abs();
        if best
            .map(|(_, best_distance)| distance < best_distance)
            .unwrap_or(true)
        {
            best = Some((step, distance));
        }
    }
    best.map(|(step, _)| (format_step(step), step / units_per_pixel))
}

fn format_step(step: f32) -> String {
    if step >= 1.0 {
        format!("{step:.0}")
    } else {
        format!("{step:.1}")
    }
}

fn load_path_data(path: &Path, display_name: String) -> Result<LoadedData, String> {
    let started = Instant::now();
    let scene = modelview_core::load_path(path).map_err(|error| format!("{error:#}"))?;
    let parse_ms = started.elapsed().as_secs_f64() * 1000.0;
    Ok(analyze_scene(scene, display_name, parse_ms))
}

fn load_bytes_data(bytes: &[u8], display_name: &str) -> Result<LoadedData, String> {
    let started = Instant::now();
    let scene =
        modelview_core::load_bytes(bytes, display_name).map_err(|error| format!("{error:#}"))?;
    let parse_ms = started.elapsed().as_secs_f64() * 1000.0;
    Ok(analyze_scene(scene, display_name.to_string(), parse_ms))
}

fn load_path_data_recovering(path: &Path, display_name: String) -> Result<LoadedData, String> {
    let started = Instant::now();
    let recovered = load_path_recovering(path).map_err(|error| format!("{error:#}"))?;
    let parse_ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut data = analyze_scene(recovered.scene, display_name, parse_ms);
    data.recovery = Some(recovered.report);
    Ok(data)
}

fn analyze_scene(scene: Scene, display_name: String, parse_ms: f64) -> LoadedData {
    let reports: Vec<MeshReport> = scene
        .objects
        .iter()
        .map(|object| analyze(&object.mesh, scene.format == Format::Stl))
        .collect();
    let summary = summarize(&reports);
    LoadedData {
        scene,
        reports,
        summary,
        parse_ms,
        display_name,
        recovery: None,
    }
}

fn save_screenshot(path: &Path, image: &egui::ColorImage, source: &str, source_path: &str) {
    let size = image.size;
    let mut rgba = Vec::with_capacity(size[0] * size[1] * 4);
    for pixel in &image.pixels {
        rgba.extend_from_slice(&pixel.to_array());
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match image::save_buffer_with_format(
        path,
        &rgba,
        size[0] as u32,
        size[1] as u32,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    ) {
        Ok(()) => println!(
            "screenshot saved: {} ({}x{})",
            path.display(),
            size[0],
            size[1]
        ),
        Err(error) => eprintln!("screenshot failed: {error}"),
    }

    let captured = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let meta = format!(
        "{{\n  \"app\": \"ModelView {}\",\n  \"captured_unix\": {captured},\n  \"size\": [{}, {}],\n  \"source\": \"{}\",\n  \"source_path\": \"{}\"\n}}\n",
        env!("CARGO_PKG_VERSION"),
        size[0],
        size[1],
        json_escape(source),
        json_escape(source_path),
    );
    let mut meta_path = path.to_path_buf();
    meta_path.set_extension("png.json");
    let _ = std::fs::write(meta_path, meta);
}

fn json_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}
