use std::fs;
use std::path::PathBuf;
use vc_core::{WorkBundle, AppSettings, Result, CoreError};

pub trait BundleStore {
    fn load(&self) -> Result<Vec<WorkBundle>>;
    fn save(&self, bundles: &[WorkBundle]) -> Result<()>;
    fn load_settings(&self) -> Result<AppSettings>;
    fn save_settings(&self, settings: &AppSettings) -> Result<()>;
}

pub struct JsonBundleStore {
    store_dir: PathBuf,
}

impl JsonBundleStore {
    pub fn new() -> Result<Self> {
        Self::new_in(Self::get_store_dir()?)
    }

    /// Create a store rooted at an explicit directory (custom location / tests).
    pub fn new_in(store_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&store_dir)
            .map_err(|e| CoreError::Internal(format!("Cannot create store dir: {}", e)))?;
        Ok(Self { store_dir })
    }

    fn get_store_dir() -> Result<PathBuf> {
        #[cfg(target_os = "macos")]
        {
            dirs::config_dir()
                .or_else(dirs::home_dir)
                .map(|p| p.join("vibe-control"))
                .ok_or_else(|| CoreError::Internal("Cannot find home directory".to_string()))
        }

        #[cfg(target_os = "windows")]
        {
            dirs::config_dir()
                .map(|p| p.join("vibe-control"))
                .ok_or_else(|| CoreError::Internal("Cannot find config directory".to_string()))
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            dirs::config_dir()
                .map(|p| p.join("vibe-control"))
                .ok_or_else(|| CoreError::Internal("Cannot find config directory".to_string()))
        }
    }

    fn bundles_path(&self) -> PathBuf {
        self.store_dir.join("bundles.json")
    }

    fn settings_path(&self) -> PathBuf {
        self.store_dir.join("settings.json")
    }

    /// Durably write `bytes` to `path` via a temp-file swap.
    ///
    /// Writes to `<path>.tmp` first, then atomically `rename`s it over `path`
    /// so a reader never observes a half-written file. On ANY failure (write or
    /// rename) the temp file is removed before returning so a crash/full-disk
    /// mid-save leaves no orphaned `.tmp` behind (see known-deviations C1/C2).
    fn atomic_write(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
        let temp_path = PathBuf::from(format!("{}.tmp", path.display()));

        if let Err(e) = fs::write(&temp_path, bytes) {
            let _ = fs::remove_file(&temp_path);
            return Err(CoreError::Corrupt(format!("Cannot write temp file: {}", e)));
        }

        if let Err(e) = fs::rename(&temp_path, path) {
            let _ = fs::remove_file(&temp_path);
            return Err(CoreError::Corrupt(format!("Cannot rename file: {}", e)));
        }

        Ok(())
    }
}

impl BundleStore for JsonBundleStore {
    fn load(&self) -> Result<Vec<WorkBundle>> {
        let path = self.bundles_path();
        if !path.exists() {
            return Ok(Vec::new());
        }

        let bytes = fs::read(&path)
            .map_err(|e| CoreError::Corrupt(format!("Cannot read store: {}", e)))?;

        vc_core::migrate::load_and_migrate(&bytes)
    }

    fn save(&self, bundles: &[WorkBundle]) -> Result<()> {
        let bytes = vc_core::migrate::serialize(bundles)?;
        Self::atomic_write(&self.bundles_path(), &bytes)
    }

    fn load_settings(&self) -> Result<AppSettings> {
        let path = self.settings_path();
        if !path.exists() {
            return Ok(AppSettings::default());
        }

        let bytes = fs::read(&path)
            .map_err(|e| CoreError::Corrupt(format!("Cannot read settings: {}", e)))?;

        serde_json::from_slice(&bytes)
            .map_err(|e| CoreError::Corrupt(format!("Cannot parse settings: {}", e)))
    }

    fn save_settings(&self, settings: &AppSettings) -> Result<()> {
        let path = self.settings_path();
        let bytes = serde_json::to_vec(settings)
            .map_err(|e| CoreError::Corrupt(format!("Cannot serialize settings: {}", e)))?;

        Self::atomic_write(&path, &bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vc_core::WorkBundle;

    /// Isolated temp store dir (never touches the real user config dir).
    fn temp_store(name: &str) -> (JsonBundleStore, PathBuf) {
        let dir = std::env::temp_dir().join(format!("vc-store-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let store = JsonBundleStore::new_in(dir.clone()).unwrap();
        (store, dir)
    }

    #[test]
    fn test_load_nonexistent() {
        let (store, dir) = temp_store("load-empty");
        let result = store.load();
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_save_load_roundtrip() {
        let (store, dir) = temp_store("roundtrip");
        let bundles = vec![WorkBundle::new("A"), WorkBundle::new("B")];
        store.save(&bundles).unwrap();

        let loaded = store.load().unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].name, "A");
        assert_eq!(loaded[1].name, "B");
        let _ = fs::remove_dir_all(&dir);
    }

    /// C1: a successful save leaves no orphaned `.tmp` file behind.
    #[test]
    fn test_save_leaves_no_temp_file() {
        let (store, dir) = temp_store("no-temp");
        store.save(&[WorkBundle::new("A")]).unwrap();
        let temp = dir.join("bundles.json.tmp");
        assert!(!temp.exists(), "temp file should be renamed away, not left behind");
        assert!(dir.join("bundles.json").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    /// C1: when the rename cannot land (store dir removed after construction),
    /// save() fails AND cleans up its temp file rather than leaking it.
    #[test]
    fn test_save_cleans_temp_on_failure() {
        let (store, dir) = temp_store("cleanup");
        // Remove the store dir so both the temp write and rename target are gone.
        fs::remove_dir_all(&dir).unwrap();
        let result = store.save(&[WorkBundle::new("A")]);
        assert!(result.is_err(), "save into a missing dir must fail");
        assert!(!dir.join("bundles.json.tmp").exists(), "temp file must be cleaned up on failure");
        let _ = fs::remove_dir_all(&dir);
    }

    /// C2: settings save is atomic (temp+rename) and roundtrips, leaving no `.tmp`.
    #[test]
    fn test_settings_atomic_roundtrip() {
        let (store, dir) = temp_store("settings");
        let settings = AppSettings { panel_width: 321.0, ..AppSettings::default() };
        store.save_settings(&settings).unwrap();

        assert!(!dir.join("settings.json.tmp").exists(), "settings temp file should be renamed away");
        let loaded = store.load_settings().unwrap();
        assert_eq!(loaded.panel_width, 321.0);
        let _ = fs::remove_dir_all(&dir);
    }
}
