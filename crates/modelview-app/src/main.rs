#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod camera;
mod gpu;
mod repair_flow;
mod settings;
mod slicer;
mod streaming;
mod theme;
mod working_file;

use eframe::egui;
use std::path::PathBuf;

struct Args {
    file: Option<PathBuf>,
    screenshot: Option<PathBuf>,
    width: u32,
    height: u32,
    help_open: bool,
    inspector_closed: bool,
    focus_demo: bool,
    theater_demo: bool,
    orientation_demo: bool,
    recover_demo: bool,
    screenshot_live: bool,
    settings_open: bool,
    repair_open: bool,
}

fn parse_args() -> Args {
    let mut args = Args {
        file: None,
        screenshot: None,
        width: 1440,
        height: 900,
        help_open: false,
        inspector_closed: false,
        focus_demo: false,
        theater_demo: false,
        orientation_demo: false,
        recover_demo: false,
        screenshot_live: false,
        settings_open: false,
        repair_open: false,
    };
    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--file" => args.file = iter.next().map(PathBuf::from),
            "--screenshot" => args.screenshot = iter.next().map(PathBuf::from),
            "--help-open" => args.help_open = true,
            "--inspector-closed" => args.inspector_closed = true,
            "--focus-demo" => args.focus_demo = true,
            "--theater-demo" => args.theater_demo = true,
            "--orientation-demo" => args.orientation_demo = true,
            "--recover-demo" => args.recover_demo = true,
            "--screenshot-live" => args.screenshot_live = true,
            "--settings-open" => args.settings_open = true,
            "--repair-open" => args.repair_open = true,
            "--size" => {
                if let Some(value) = iter.next()
                    && let Some((w, h)) = value.split_once('x')
                    && let (Ok(w), Ok(h)) = (w.parse(), h.parse())
                {
                    args.width = w;
                    args.height = h;
                }
            }
            "--help" | "-h" => {
                println!(
                    "ModelView — fast STL and 3MF printability inspector\n\n\
                     USAGE:\n  modelview [FILE] [OPTIONS]\n\n\
                     OPTIONS:\n  --file <path>        Open an STL or 3MF on launch\n  \
                     --screenshot <path>  Render the first viewport to a PNG, then exit\n  \
                     --size <WxH>         Window size in points (default 1440x900)\n  \
                     --help-open          Open the controls overlay on launch\n  \
                     --inspector-closed   Start with the inspector hidden\n  \
                     --focus-demo         Focus the first check row on launch\n  \
                     --theater-demo       Open defect theater on the largest defect\n  \
                     --orientation-demo   Open the orientation adviser on launch\n  \
                     --recover-demo       Auto-recover a recoverable large 3MF on load\n  \
                     --screenshot-live    Capture a progressive load before it settles\n  \
                     --settings-open      Open slicer settings on launch\n  \
                     --repair-open        Open the repair sheet for a broken sample\n  \
                     -h, --help           Print this help\n"
                );
                std::process::exit(0);
            }
            other => {
                if !other.starts_with('-') && args.file.is_none() {
                    args.file = Some(PathBuf::from(other));
                }
            }
        }
    }
    args
}

fn load_icon() -> Option<egui::IconData> {
    let image = image::load_from_memory(include_bytes!("../assets/icon.png"))
        .ok()?
        .to_rgba8();
    let (width, height) = image.dimensions();
    Some(egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    })
}

fn main() -> eframe::Result {
    let args = parse_args();
    let screenshot_mode = args.screenshot.is_some();
    let screenshot_persistence = screenshot_mode
        .then(|| std::env::temp_dir().join(format!("modelview-screenshot-{}", std::process::id())));
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("ModelView")
        .with_inner_size([args.width as f32, args.height as f32])
        .with_min_inner_size([1024.0, 680.0])
        .with_app_id("modelview");
    if let Some(icon) = load_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        persist_window: !screenshot_mode,
        persistence_path: screenshot_persistence,
        ..Default::default()
    };
    let file = args.file;
    let screenshot = args.screenshot.map(|path| app::ScreenshotConfig {
        path,
        delay_frames: 12,
        allow_loading: args.screenshot_live,
    });
    let help_open = args.help_open;
    let inspector_open = !args.inspector_closed;
    let focus_demo = args.focus_demo;
    let theater_demo = args.theater_demo;
    let orientation_demo = args.orientation_demo;
    let recover_demo = args.recover_demo;
    let settings_open = args.settings_open;
    let repair_open = args.repair_open;
    eframe::run_native(
        "ModelView",
        options,
        Box::new(move |cc| {
            Ok(Box::new(app::ModelView::new(
                cc,
                file,
                screenshot,
                help_open,
                inspector_open,
                focus_demo,
                theater_demo,
                orientation_demo,
                recover_demo,
                settings_open,
                repair_open,
            )))
        }),
    )
}
