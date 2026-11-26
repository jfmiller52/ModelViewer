use crate::slicer::SlicerChoice;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    pub slicer: Option<SlicerChoice>,
    pub last_open_dir: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slicer::SlicerKind;

    #[derive(Default)]
    struct MemoryStorage(std::collections::HashMap<String, String>);

    impl eframe::Storage for MemoryStorage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }

        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.to_string(), value);
        }

        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }

        fn flush(&mut self) {}
    }

    fn example() -> Settings {
        Settings {
            slicer: Some(SlicerChoice {
                name: "OrcaSlicer".into(),
                kind: SlicerKind::Orca,
                path: PathBuf::from("/Applications/OrcaSlicer.app"),
            }),
            last_open_dir: Some(PathBuf::from("/Users/me/Models")),
        }
    }

    #[test]
    fn settings_round_trip() {
        let settings = example();
        let text = serde_json::to_string(&settings).unwrap();
        let back: Settings = serde_json::from_str(&text).unwrap();
        assert_eq!(back.slicer, settings.slicer);
        assert_eq!(back.last_open_dir, settings.last_open_dir);
    }

    #[test]
    fn settings_round_trip_through_eframe_storage() {
        let settings = example();
        let mut storage = MemoryStorage::default();
        eframe::set_value(&mut storage, eframe::APP_KEY, &settings);
        let back: Settings = eframe::get_value(&storage, eframe::APP_KEY).unwrap();
        assert_eq!(back.slicer, settings.slicer);
        assert_eq!(back.last_open_dir, settings.last_open_dir);
    }
}
