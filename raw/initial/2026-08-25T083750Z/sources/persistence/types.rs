//! Common types shared between storage backends.

use serde::{Deserialize, Serialize};
use shared::frontend::dto::{ApiError, MutationId, MutationIntentDto};
use shared::{
    Equipment, Exercise, ExerciseId, MuscleGroup, SessionExercise, SessionId, SessionStatus,
    TemplateExercise, TemplateId, UserId, WorkoutSession, WorkoutTemplate,
};
use uuid::Uuid;

/// Error type for store operations.
pub type StoreError = String;
pub type StoreResult<T> = Result<T, StoreError>;

/// Serializable outbox record for storage.
///
/// Stores mutation intents waiting to be synced to the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxRecord {
    pub mutation_id: String,
    pub method: String,
    pub path: String,
    pub body: String,
    pub created_at: i64,
}

impl OutboxRecord {
    /// Create a new outbox record from a mutation intent DTO.
    pub fn from_intent(intent: &MutationIntentDto) -> StoreResult<Self> {
        let body = serde_json::to_string(&intent.body)
            .map_err(|e| format!("Failed to serialize mutation body: {}", e))?;

        Ok(Self {
            mutation_id: intent.mutation_id.as_str(),
            method: intent.method.clone(),
            path: intent.path.clone(),
            body,
            created_at: intent.client_datetime.timestamp_millis(),
        })
    }

    /// Get the mutation ID as a MutationId.
    pub fn mutation_id(&self) -> StoreResult<MutationId> {
        MutationId::from_string(&self.mutation_id)
            .map_err(|_| format!("Invalid mutation_id: {}", self.mutation_id))
    }

    /// Rehydrate an outbox record back into a mutation intent DTO.
    pub fn to_intent(&self) -> StoreResult<MutationIntentDto> {
        let body = serde_json::from_str(&self.body)
            .map_err(|e| format!("Failed to deserialize mutation body: {}", e))?;
        let mutation_id = self.mutation_id()?;
        let client_datetime = chrono::DateTime::from_timestamp_millis(self.created_at)
            .ok_or_else(|| format!("Invalid created_at: {}", self.created_at))?;

        Ok(MutationIntentDto {
            mutation_id,
            method: self.method.clone(),
            path: self.path.clone(),
            client_datetime,
            body,
        })
    }
}

/// Serializable dead letter record for storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadLetterRecord {
    pub mutation_id: String,
    pub method: String,
    pub path: String,
    pub body: String,
    pub created_at: i64,
    pub rejected_at: i64,
    pub error: Option<String>,
}

impl DeadLetterRecord {
    /// Create a new dead letter record from an outbox record and error.
    pub fn new(
        mutation_id: String,
        method: String,
        path: String,
        body: String,
        created_at: i64,
        error: Option<&ApiError>,
    ) -> Self {
        let rejected_at = chrono::Utc::now().timestamp_millis();
        let error_json = error.and_then(|e| serde_json::to_string(e).ok());

        Self {
            mutation_id,
            method,
            path,
            body,
            created_at,
            rejected_at,
            error: error_json,
        }
    }
}

// ============================================================================
// Workout Storage Records
// ============================================================================

/// Serializable exercise record for storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExerciseRecord {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub muscle_groups: String, // JSON array of MuscleGroup
    pub equipment: String,
    pub is_custom: bool,
    pub is_favorite: bool,
    pub owner_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<&Exercise> for ExerciseRecord {
    fn from(exercise: &Exercise) -> Self {
        Self {
            id: exercise.id.to_string(),
            name: exercise.name.clone(),
            description: exercise.description.clone(),
            muscle_groups: serde_json::to_string(&exercise.muscle_groups).unwrap_or_default(),
            equipment: serde_json::to_string(&exercise.equipment).unwrap_or_default(),
            is_custom: exercise.is_custom,
            is_favorite: exercise.is_favorite,
            owner_id: exercise.owner_id.as_ref().map(|id| id.to_string()),
            created_at: exercise.created_at.timestamp_millis(),
            updated_at: exercise.updated_at.timestamp_millis(),
        }
    }
}

impl ExerciseRecord {
    /// Convert a stored record to an Exercise.
    pub fn to_exercise(&self) -> StoreResult<Exercise> {
        let id = ExerciseId::from_string(&self.id)
            .map_err(|_| format!("Invalid exercise id: {}", self.id))?;

        let owner_id = self
            .owner_id
            .as_ref()
            .map(|s| Uuid::parse_str(s).map(UserId::from_uuid))
            .transpose()
            .map_err(|_| "Invalid owner_id".to_string())?;

        let muscle_groups: Vec<MuscleGroup> =
            serde_json::from_str(&self.muscle_groups).unwrap_or_default();

        let equipment: Equipment =
            serde_json::from_str(&self.equipment).map_err(|_| "Invalid equipment".to_string())?;

        let created_at = chrono::DateTime::from_timestamp_millis(self.created_at)
            .ok_or_else(|| "Invalid created_at".to_string())?;

        let updated_at = chrono::DateTime::from_timestamp_millis(self.updated_at)
            .ok_or_else(|| "Invalid updated_at".to_string())?;

        Ok(Exercise {
            id,
            name: self.name.clone(),
            description: self.description.clone(),
            muscle_groups,
            equipment,
            is_custom: self.is_custom,
            is_favorite: self.is_favorite,
            visibility: shared::Visibility::default(),
            owner_id,
            created_at,
            updated_at,
        })
    }
}

/// Serializable template record for storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateRecord {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    pub description: Option<String>,
    pub exercises: String, // JSON array of TemplateExercise
    pub estimated_minutes: Option<i64>,
    pub is_favorite: bool,
    pub tags: String, // JSON array of strings
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<&WorkoutTemplate> for TemplateRecord {
    fn from(template: &WorkoutTemplate) -> Self {
        Self {
            id: template.id.to_string(),
            owner_id: template.owner_id.to_string(),
            name: template.name.clone(),
            description: template.description.clone(),
            exercises: serde_json::to_string(&template.exercises).unwrap_or_default(),
            estimated_minutes: template.estimated_minutes.map(|m| m as i64),
            is_favorite: template.is_favorite,
            tags: serde_json::to_string(&template.tags).unwrap_or_default(),
            created_at: template.created_at.timestamp_millis(),
            updated_at: template.updated_at.timestamp_millis(),
        }
    }
}

impl TemplateRecord {
    /// Convert a stored record to a WorkoutTemplate.
    pub fn to_template(&self) -> StoreResult<WorkoutTemplate> {
        let id = TemplateId::from_string(&self.id)
            .map_err(|_| format!("Invalid template id: {}", self.id))?;

        let owner_id = Uuid::parse_str(&self.owner_id)
            .map(UserId::from_uuid)
            .map_err(|_| "Invalid owner_id".to_string())?;

        let exercises: Vec<TemplateExercise> =
            serde_json::from_str(&self.exercises).unwrap_or_default();

        let tags: Vec<String> = serde_json::from_str(&self.tags).unwrap_or_default();

        let created_at = chrono::DateTime::from_timestamp_millis(self.created_at)
            .ok_or_else(|| "Invalid created_at".to_string())?;

        let updated_at = chrono::DateTime::from_timestamp_millis(self.updated_at)
            .ok_or_else(|| "Invalid updated_at".to_string())?;

        Ok(WorkoutTemplate {
            id,
            owner_id,
            name: self.name.clone(),
            description: self.description.clone(),
            exercises,
            estimated_minutes: self.estimated_minutes.map(|m| m as u16),
            is_favorite: self.is_favorite,
            tags,
            created_at,
            updated_at,
        })
    }
}

/// Serializable workout session record for storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub id: String,
    pub owner_id: String,
    pub template_id: Option<String>,
    pub name: String,
    pub status: String,
    pub exercises: String, // JSON array of SessionExercise
    pub started_at: i64,
    pub paused_at: Option<i64>,
    pub total_paused_seconds: i64,
    pub notes: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<&WorkoutSession> for SessionRecord {
    fn from(session: &WorkoutSession) -> Self {
        Self {
            id: session.id.to_string(),
            owner_id: session.owner_id.to_string(),
            template_id: session.template_id.as_ref().map(|t| t.to_string()),
            name: session.name.clone(),
            status: serde_json::to_string(&session.status).unwrap_or_default(),
            exercises: serde_json::to_string(&session.exercises).unwrap_or_default(),
            started_at: session.started_at.timestamp_millis(),
            paused_at: session.paused_at.map(|t| t.timestamp_millis()),
            total_paused_seconds: session.total_paused_seconds,
            notes: session.notes.clone(),
            created_at: session.created_at.timestamp_millis(),
            updated_at: session.updated_at.timestamp_millis(),
        }
    }
}

impl SessionRecord {
    /// Convert a stored record to a WorkoutSession.
    pub fn to_session(&self) -> StoreResult<WorkoutSession> {
        let id = SessionId::from_string(&self.id)
            .map_err(|_| format!("Invalid session id: {}", self.id))?;

        let owner_id = Uuid::parse_str(&self.owner_id)
            .map(UserId::from_uuid)
            .map_err(|_| "Invalid owner_id".to_string())?;

        let template_id = self
            .template_id
            .as_ref()
            .map(|s| TemplateId::from_string(s))
            .transpose()
            .map_err(|_| "Invalid template_id".to_string())?;

        let status: SessionStatus = serde_json::from_str(&self.status).unwrap_or_default();

        let exercises: Vec<SessionExercise> =
            serde_json::from_str(&self.exercises).unwrap_or_default();

        let started_at = chrono::DateTime::from_timestamp_millis(self.started_at)
            .ok_or_else(|| "Invalid started_at".to_string())?;

        let paused_at = self
            .paused_at
            .and_then(chrono::DateTime::from_timestamp_millis);

        let created_at = chrono::DateTime::from_timestamp_millis(self.created_at)
            .ok_or_else(|| "Invalid created_at".to_string())?;

        let updated_at = chrono::DateTime::from_timestamp_millis(self.updated_at)
            .ok_or_else(|| "Invalid updated_at".to_string())?;

        Ok(WorkoutSession {
            id,
            owner_id,
            template_id,
            name: self.name.clone(),
            started_at,
            exercises,
            status,
            notes: self.notes.clone(),
            created_at,
            updated_at,
            paused_at,
            total_paused_seconds: self.total_paused_seconds,
        })
    }
}
