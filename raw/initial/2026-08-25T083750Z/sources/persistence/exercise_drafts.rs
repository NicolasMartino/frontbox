//! Exercise draft persistence.
//!
//! Provides cross-platform local draft storage for exercise create/update/translation flows.

use shared_ui::screens::ExerciseDraft;

use super::StoreResult;

#[cfg(target_arch = "wasm32")]
mod platform {
    use super::*;

    const STORAGE_PREFIX: &str = "repforge.exercise_draft.";

    fn namespaced_key(key: &str) -> String {
        format!("{}{}", STORAGE_PREFIX, key)
    }

    pub async fn load_exercise_draft(key: &str) -> StoreResult<Option<ExerciseDraft>> {
        let storage = web_sys::window()
            .ok_or("No window object")?
            .local_storage()
            .map_err(|_| "Failed to access localStorage")?
            .ok_or("localStorage not available")?;

        let value = storage
            .get_item(&namespaced_key(key))
            .map_err(|_| "Failed to read exercise draft from localStorage")?;

        match value {
            Some(payload) => {
                let draft = serde_json::from_str::<ExerciseDraft>(&payload)
                    .map_err(|e| format!("Failed to parse exercise draft: {}", e))?;
                Ok(Some(draft))
            }
            None => Ok(None),
        }
    }

    pub async fn save_exercise_draft(key: &str, draft: &ExerciseDraft) -> StoreResult<()> {
        let storage = web_sys::window()
            .ok_or("No window object")?
            .local_storage()
            .map_err(|_| "Failed to access localStorage")?
            .ok_or("localStorage not available")?;

        let payload = serde_json::to_string(draft)
            .map_err(|e| format!("Failed to serialize exercise draft: {}", e))?;

        storage
            .set_item(&namespaced_key(key), &payload)
            .map_err(|_| "Failed to save exercise draft to localStorage".to_string())
    }

    pub async fn clear_exercise_draft(key: &str) -> StoreResult<()> {
        let storage = web_sys::window()
            .ok_or("No window object")?
            .local_storage()
            .map_err(|_| "Failed to access localStorage")?
            .ok_or("localStorage not available")?;

        storage
            .remove_item(&namespaced_key(key))
            .map_err(|_| "Failed to clear exercise draft from localStorage".to_string())
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod platform {
    use super::*;

    use std::fs;
    use std::path::PathBuf;
    #[cfg(test)]
    use std::sync::{Mutex, OnceLock};

    #[cfg(test)]
    fn test_drafts_dir_override() -> &'static Mutex<Option<PathBuf>> {
        static OVERRIDE: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
        OVERRIDE.get_or_init(|| Mutex::new(None))
    }

    fn drafts_dir() -> Option<PathBuf> {
        #[cfg(test)]
        if let Some(path) = test_drafts_dir_override()
            .lock()
            .expect("test drafts override lock")
            .clone()
        {
            return Some(path);
        }

        #[cfg(target_os = "macos")]
        {
            dirs::data_dir().map(|p| p.join("com.repforge.app").join("exercise-drafts"))
        }

        #[cfg(target_os = "linux")]
        {
            dirs::config_dir().map(|p| p.join("repforge").join("exercise-drafts"))
        }

        #[cfg(target_os = "windows")]
        {
            dirs::data_local_dir().map(|p| p.join("RepForge").join("exercise-drafts"))
        }

        #[cfg(target_os = "android")]
        {
            let base = std::env::var("HOME")
                .or_else(|_| std::env::var("TMPDIR"))
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("/data/local/tmp"));
            Some(base.join("exercise-drafts"))
        }

        #[cfg(target_os = "ios")]
        {
            dirs::document_dir().map(|p| p.join("exercise-drafts"))
        }
    }

    pub(super) fn key_to_filename(key: &str) -> String {
        key.chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
            .collect()
    }

    fn draft_file(key: &str) -> Option<PathBuf> {
        drafts_dir().map(|dir| dir.join(format!("{}.json", key_to_filename(key))))
    }

    pub async fn load_exercise_draft(key: &str) -> StoreResult<Option<ExerciseDraft>> {
        let Some(path) = draft_file(key) else {
            return Err("Could not determine exercise draft path".to_string());
        };

        if !path.exists() {
            return Ok(None);
        }

        let payload = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read draft file {}: {}", path.display(), e))?;
        let draft = serde_json::from_str::<ExerciseDraft>(&payload)
            .map_err(|e| format!("Failed to parse draft file {}: {}", path.display(), e))?;

        Ok(Some(draft))
    }

    pub async fn save_exercise_draft(key: &str, draft: &ExerciseDraft) -> StoreResult<()> {
        let Some(path) = draft_file(key) else {
            return Err("Could not determine exercise draft path".to_string());
        };

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "Failed to create exercise draft directory {}: {}",
                    parent.display(),
                    e
                )
            })?;
        }

        let payload = serde_json::to_string_pretty(draft)
            .map_err(|e| format!("Failed to serialize exercise draft: {}", e))?;

        fs::write(&path, payload)
            .map_err(|e| format!("Failed to write draft file {}: {}", path.display(), e))
    }

    pub async fn clear_exercise_draft(key: &str) -> StoreResult<()> {
        let Some(path) = draft_file(key) else {
            return Err("Could not determine exercise draft path".to_string());
        };

        if !path.exists() {
            return Ok(());
        }

        fs::remove_file(&path)
            .map_err(|e| format!("Failed to remove draft file {}: {}", path.display(), e))
    }

    #[cfg(test)]
    pub(super) fn set_test_drafts_dir(path: Option<PathBuf>) {
        *test_drafts_dir_override()
            .lock()
            .expect("test drafts override lock") = path;
    }
}

pub use platform::{clear_exercise_draft, load_exercise_draft, save_exercise_draft};

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn sample_draft() -> ExerciseDraft {
        ExerciseDraft {
            exercise_id: Some(Uuid::new_v4()),
            name: Some("Bench Press".to_string()),
            muscle_groups: vec!["chest".to_string(), "triceps".to_string()],
            equipment: Some("barbell".to_string()),
            description: Some("Classic press".to_string()),
            instructions: Some("Brace and press".to_string()),
            video_url: Some("https://example.com/bench".to_string()),
            visibility: shared_ui::screens::ExerciseDraftVisibility::Unlisted,
            create_dispatched: true,
            last_create_mutation_id: Some(Uuid::new_v4().to_string()),
            last_updated_at_ms: Some(1234),
        }
    }

    #[test]
    fn native_key_to_filename_replaces_non_alphanumeric_characters() {
        assert_eq!(
            platform::key_to_filename("exercise-draft:user@example.com/update"),
            "exercise_draft_user_example_com_update"
        );
    }

    #[tokio::test]
    async fn native_exercise_draft_round_trip_and_clear() {
        let base = std::env::temp_dir().join(format!(
            "repforge-exercise-draft-tests-{}",
            Uuid::new_v4().simple()
        ));
        platform::set_test_drafts_dir(Some(base.clone()));

        let key = "exercise-draft:bench/update";
        let draft = sample_draft();

        assert_eq!(
            load_exercise_draft(key).await.expect("load should succeed"),
            None
        );

        save_exercise_draft(key, &draft)
            .await
            .expect("save should succeed");
        let expected_path = base.join(format!("{}.json", platform::key_to_filename(key)));
        assert!(expected_path.exists(), "draft file should be written");

        let loaded = load_exercise_draft(key)
            .await
            .expect("load after save should succeed");
        assert_eq!(loaded, Some(draft.clone()));

        clear_exercise_draft(key)
            .await
            .expect("clear should succeed");
        assert_eq!(
            load_exercise_draft(key)
                .await
                .expect("load after clear should succeed"),
            None
        );

        platform::set_test_drafts_dir(None);
    }
}
