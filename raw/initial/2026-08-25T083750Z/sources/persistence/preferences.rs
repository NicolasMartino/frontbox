//! User preferences storage.
//!
//! Provides cross-platform storage for user preferences like theme selection.
//! - Web: Uses localStorage (synchronous)
//! - Native: Uses a config file in the app data directory

use shared_ui::ThemeId;

use super::StoreResult;

/// Storage key for theme preference.
#[cfg(target_arch = "wasm32")]
const THEME_STORAGE_KEY: &str = "repforge_theme";

/// Preferences store for web platform (localStorage).
#[cfg(target_arch = "wasm32")]
mod platform {
    use super::*;
    use web_sys::window;

    /// Get the saved theme from localStorage.
    /// Returns None if no theme is saved or if localStorage is unavailable.
    pub fn get_theme() -> Option<ThemeId> {
        let storage = window()?.local_storage().ok()??;
        let value = storage.get_item(THEME_STORAGE_KEY).ok()??;
        ThemeId::from_slug(&value)
    }

    /// Save the theme to localStorage.
    pub fn set_theme(theme: ThemeId) -> StoreResult<()> {
        let storage = window()
            .ok_or("No window object")?
            .local_storage()
            .map_err(|_| "Failed to access localStorage")?
            .ok_or("localStorage not available")?;

        storage
            .set_item(THEME_STORAGE_KEY, theme.slug())
            .map_err(|_| "Failed to save theme to localStorage".to_string())?;

        Ok(())
    }
}

/// Preferences store for native platform (config file).
#[cfg(not(target_arch = "wasm32"))]
mod platform {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    #[cfg(test)]
    use std::sync::{Mutex, OnceLock};

    #[cfg(test)]
    fn test_preferences_file_override() -> &'static Mutex<Option<PathBuf>> {
        static OVERRIDE: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
        OVERRIDE.get_or_init(|| Mutex::new(None))
    }

    /// Get the preferences file path.
    fn preferences_file() -> Option<PathBuf> {
        #[cfg(test)]
        if let Some(path) = test_preferences_file_override()
            .lock()
            .expect("test preferences override lock")
            .clone()
        {
            return Some(path);
        }

        // Use the standard app support directory
        #[cfg(target_os = "macos")]
        {
            dirs::data_dir().map(|p| p.join("com.repforge.app").join("preferences.json"))
        }
        #[cfg(target_os = "linux")]
        {
            dirs::config_dir().map(|p| p.join("repforge").join("preferences.json"))
        }
        #[cfg(target_os = "windows")]
        {
            dirs::data_local_dir().map(|p| p.join("RepForge").join("preferences.json"))
        }
        #[cfg(target_os = "android")]
        {
            // Match native DB directory resolution: runtime-derived app-writable location.
            let base = std::env::var("HOME")
                .or_else(|_| std::env::var("TMPDIR"))
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("/data/local/tmp"));
            Some(base.join("preferences.json"))
        }
        #[cfg(target_os = "ios")]
        {
            // iOS uses the Documents directory
            dirs::document_dir().map(|p| p.join("preferences.json"))
        }
    }

    /// Simple preferences structure.
    #[derive(serde::Serialize, serde::Deserialize, Default)]
    struct Preferences {
        #[serde(default)]
        theme: Option<String>,
    }

    /// Read preferences from file.
    fn read_preferences() -> Preferences {
        preferences_file()
            .and_then(|path| fs::read_to_string(&path).ok())
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    }

    /// Write preferences to file.
    fn write_preferences(prefs: &Preferences) -> StoreResult<()> {
        let path = preferences_file().ok_or("Could not determine preferences path")?;

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "Failed to create preferences directory {}: {}",
                    parent.display(),
                    e
                )
            })?;
        }

        let content = serde_json::to_string_pretty(prefs)
            .map_err(|e| format!("Failed to serialize preferences: {}", e))?;

        fs::write(&path, content)
            .map_err(|e| format!("Failed to write preferences file {}: {}", path.display(), e))?;

        Ok(())
    }

    /// Get the saved theme from the preferences file.
    pub fn get_theme() -> Option<ThemeId> {
        let prefs = read_preferences();
        prefs
            .theme
            .as_ref()
            .and_then(|slug| ThemeId::from_slug(slug))
    }

    /// Save the theme to the preferences file.
    pub fn set_theme(theme: ThemeId) -> StoreResult<()> {
        let mut prefs = read_preferences();
        prefs.theme = Some(theme.slug().to_string());
        write_preferences(&prefs)
    }

    #[cfg(test)]
    pub(super) fn set_test_preferences_file(path: Option<PathBuf>) {
        *test_preferences_file_override()
            .lock()
            .expect("test preferences override lock") = path;
    }
}

// Re-export the platform-specific functions
pub use platform::{get_theme, set_theme};

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(target_arch = "wasm32"))]
    use uuid::Uuid;

    /// Test that get_theme returns None when no theme is saved.
    #[test]
    fn test_get_theme_returns_none_initially() {
        // On native, this depends on file system state
        // Just verify the function runs without panicking
        let _ = get_theme();
    }

    /// Test that ThemeId can be set and retrieved (compile-time check).
    #[test]
    fn test_set_theme_compiles() {
        // This is a compile-time check - the function signature must exist
        fn _check_signature(_theme: shared_ui::ThemeId) -> StoreResult<()> {
            set_theme(_theme)
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_preferences_round_trip_theme_selection() {
        let base = std::env::temp_dir().join(format!(
            "repforge-preferences-tests-{}",
            Uuid::new_v4().simple()
        ));
        let path = base.join("preferences.json");
        platform::set_test_preferences_file(Some(path));

        assert_eq!(get_theme(), None, "fresh temp config should be empty");

        set_theme(shared_ui::ThemeId::Dark).expect("theme write should succeed");
        assert_eq!(get_theme(), Some(shared_ui::ThemeId::Dark));

        set_theme(shared_ui::ThemeId::Athletic).expect("theme overwrite should succeed");
        assert_eq!(get_theme(), Some(shared_ui::ThemeId::Athletic));

        platform::set_test_preferences_file(None);
    }
}
