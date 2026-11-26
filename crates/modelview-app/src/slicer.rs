use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SlicerKind {
    Bambu,
    Orca,
    Prusa,
    Cura,
    Custom,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlicerChoice {
    pub name: String,
    pub kind: SlicerKind,
    pub path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlicerCandidate {
    pub name: String,
    pub kind: SlicerKind,
    pub path: PathBuf,
}

const APPS: [(&str, SlicerKind, &str); 4] = [
    ("Bambu Studio", SlicerKind::Bambu, "BambuStudio.app"),
    ("OrcaSlicer", SlicerKind::Orca, "OrcaSlicer.app"),
    ("PrusaSlicer", SlicerKind::Prusa, "PrusaSlicer.app"),
    ("UltiMaker Cura", SlicerKind::Cura, "UltiMaker Cura.app"),
];

const EXES: [(&str, SlicerKind, &str); 4] = [
    (
        "Bambu Studio",
        SlicerKind::Bambu,
        "Bambu Studio/bambu-studio.exe",
    ),
    ("OrcaSlicer", SlicerKind::Orca, "OrcaSlicer/orca-slicer.exe"),
    (
        "PrusaSlicer",
        SlicerKind::Prusa,
        "Prusa3D/PrusaSlicer/prusa-slicer.exe",
    ),
    (
        "UltiMaker Cura",
        SlicerKind::Cura,
        "UltiMaker Cura/UltiMaker-Cura.exe",
    ),
];

const COMMANDS: [(&str, SlicerKind, &str); 4] = [
    ("Bambu Studio", SlicerKind::Bambu, "bambu-studio"),
    ("OrcaSlicer", SlicerKind::Orca, "orca-slicer"),
    ("PrusaSlicer", SlicerKind::Prusa, "prusa-slicer"),
    ("UltiMaker Cura", SlicerKind::Cura, "cura"),
];

pub fn detect_slicers() -> Vec<SlicerCandidate> {
    let mut roots = Vec::new();
    #[cfg(target_os = "macos")]
    {
        roots.push(PathBuf::from("/Applications"));
        if let Some(home) = std::env::var_os("HOME") {
            roots.push(PathBuf::from(home).join("Applications"));
        }
    }
    #[cfg(target_os = "windows")]
    {
        for key in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
            if let Some(value) = std::env::var_os(key) {
                roots.push(PathBuf::from(value));
            }
        }
        if let Some(value) = std::env::var_os("LOCALAPPDATA") {
            roots.push(PathBuf::from(value).join("Programs"));
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(path) = std::env::var_os("PATH") {
            roots.extend(std::env::split_paths(&path));
        }
    }
    detect_slicers_in(&roots)
}

pub fn detect_slicers_in(roots: &[PathBuf]) -> Vec<SlicerCandidate> {
    let mut found: Vec<SlicerCandidate> = Vec::new();
    let mut push = |name: &str, kind: SlicerKind, path: PathBuf| {
        if path.exists() && !found.iter().any(|c| c.kind == kind && c.path == path) {
            found.push(SlicerCandidate {
                name: name.to_string(),
                kind,
                path,
            });
        }
    };
    for root in roots {
        for (name, kind, app) in APPS {
            push(name, kind, root.join(app));
        }
        for (name, kind, exe) in EXES {
            push(name, kind, root.join(exe));
        }
        for (name, kind, command) in COMMANDS {
            push(name, kind, root.join(command));
        }
    }
    #[cfg(target_os = "windows")]
    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_cura_folder = path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.to_ascii_lowercase().starts_with("ultimaker cura"))
                .unwrap_or(false);
            if is_cura_folder {
                push(
                    "UltiMaker Cura",
                    SlicerKind::Cura,
                    path.join("UltiMaker-Cura.exe"),
                );
            }
        }
    }
    found
}

pub fn launch_command(choice: &SlicerChoice, file: &Path) -> std::io::Result<Command> {
    #[cfg(target_os = "macos")]
    {
        let is_app = choice
            .path
            .extension()
            .map(|ext| ext.eq_ignore_ascii_case("app"))
            .unwrap_or(false)
            || choice.path.is_dir();
        if is_app {
            let mut command = Command::new("/usr/bin/open");
            command.arg("-a").arg(&choice.path).arg(file);
            return Ok(command);
        }
    }
    let mut command = Command::new(&choice.path);
    command.arg(file);
    Ok(command)
}

pub fn launch(choice: &SlicerChoice, file: &Path) -> std::io::Result<()> {
    if !choice.path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("slicer not found: {}", choice.path.display()),
        ));
    }
    launch_command(choice, file)?.spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(1);

    fn temp_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "modelview-slicer-{}-{}-{label}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn detects_fake_apps_under_a_root() {
        let root = temp_dir("detect");
        let app = root.join("BambuStudio.app");
        std::fs::create_dir_all(&app).unwrap();
        let found = detect_slicers_in(std::slice::from_ref(&root));
        assert!(
            found
                .iter()
                .any(|c| c.kind == SlicerKind::Bambu && c.path == app)
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn detects_fake_exe_under_a_root() {
        let root = temp_dir("detect-win");
        let vendor = root.join("Bambu Studio");
        std::fs::create_dir_all(&vendor).unwrap();
        let exe = vendor.join("bambu-studio.exe");
        std::fs::write(&exe, b"fake").unwrap();
        let found = detect_slicers_in(std::slice::from_ref(&root));
        assert!(
            found
                .iter()
                .any(|c| c.kind == SlicerKind::Bambu && c.path == exe)
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn detects_versioned_cura_folder() {
        let root = temp_dir("detect-cura");
        let folder = root.join("UltiMaker Cura 5.7.0");
        std::fs::create_dir_all(&folder).unwrap();
        let exe = folder.join("UltiMaker-Cura.exe");
        std::fs::write(&exe, b"fake").unwrap();
        let found = detect_slicers_in(std::slice::from_ref(&root));
        assert!(
            found
                .iter()
                .any(|c| c.kind == SlicerKind::Cura && c.path == exe)
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn mac_app_bundle_uses_open_with_separate_arguments() {
        let choice = SlicerChoice {
            name: "Bambu Studio".into(),
            kind: SlicerKind::Bambu,
            path: PathBuf::from("/Applications/Bambu Studio.app"),
        };
        let file = PathBuf::from("/tmp/model with spaces.stl");
        let command = launch_command(&choice, &file).unwrap();
        assert_eq!(command.get_program(), "/usr/bin/open");
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            vec![
                "-a",
                "/Applications/Bambu Studio.app",
                "/tmp/model with spaces.stl"
            ]
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn missing_app_bundle_reports_an_error() {
        let choice = SlicerChoice {
            name: "Missing".into(),
            kind: SlicerKind::Custom,
            path: std::env::temp_dir().join("modelview-definitely-missing.app"),
        };
        assert!(launch(&choice, Path::new("/tmp/model.stl")).is_err());
    }

    #[test]
    fn missing_slicer_path_reports_a_spawn_error() {
        let choice = SlicerChoice {
            name: "Missing".into(),
            kind: SlicerKind::Custom,
            path: PathBuf::from("/definitely/missing/modelview-slicer"),
        };
        assert!(launch(&choice, Path::new("/tmp/model.stl")).is_err());
    }

    #[test]
    fn bare_executable_receives_the_file_argument() {
        let choice = SlicerChoice {
            name: "My Slicer".into(),
            kind: SlicerKind::Custom,
            path: PathBuf::from("/opt/My Slicer/slicer"),
        };
        let command = launch_command(&choice, Path::new("/tmp/胸像.stl")).unwrap();
        assert_eq!(command.get_program(), "/opt/My Slicer/slicer");
        assert_eq!(command.get_args().count(), 1);
    }
}
