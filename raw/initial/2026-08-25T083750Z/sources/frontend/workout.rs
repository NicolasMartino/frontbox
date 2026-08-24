//! Workout domain types
//!
//! Core business entities for workout tracking functionality.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

// ============================================================================
// IDs
// ============================================================================

/// Unique identifier for an Exercise definition.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
#[schema(value_type = String)]
pub struct ExerciseId(Uuid);

impl ExerciseId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_string(id: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(id)?))
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }

    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl Default for ExerciseId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Uuid> for ExerciseId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl std::fmt::Display for ExerciseId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for ExerciseId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Unique identifier for a Workout Template.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
#[schema(value_type = String)]
pub struct TemplateId(Uuid);

impl TemplateId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_string(id: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(id)?))
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }

    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl Default for TemplateId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Uuid> for TemplateId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl std::fmt::Display for TemplateId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for TemplateId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Unique identifier for a Workout Session.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
#[schema(value_type = String)]
pub struct SessionId(Uuid);

impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_string(id: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(id)?))
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }

    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Uuid> for SessionId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for SessionId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Unique identifier for a Workout Set.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
#[schema(value_type = String)]
pub struct SetId(Uuid);

impl SetId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_string(id: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(id)?))
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }

    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl Default for SetId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Uuid> for SetId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl std::fmt::Display for SetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for SetId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

// ============================================================================
// Enums
// ============================================================================

/// Muscle groups that an exercise targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MuscleGroup {
    // Upper Body
    Chest,
    Back,
    Shoulders,
    Biceps,
    Triceps,
    Forearms,
    // Core
    Abs,
    Obliques,
    LowerBack,
    // Lower Body
    Quads,
    Hamstrings,
    Glutes,
    Calves,
    // Full Body
    FullBody,
}

impl MuscleGroup {
    /// Get all muscle groups.
    pub fn all() -> &'static [MuscleGroup] {
        &[
            Self::Chest,
            Self::Back,
            Self::Shoulders,
            Self::Biceps,
            Self::Triceps,
            Self::Forearms,
            Self::Abs,
            Self::Obliques,
            Self::LowerBack,
            Self::Quads,
            Self::Hamstrings,
            Self::Glutes,
            Self::Calves,
            Self::FullBody,
        ]
    }

    /// Get display name for the muscle group.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Chest => "Chest",
            Self::Back => "Back",
            Self::Shoulders => "Shoulders",
            Self::Biceps => "Biceps",
            Self::Triceps => "Triceps",
            Self::Forearms => "Forearms",
            Self::Abs => "Abs",
            Self::Obliques => "Obliques",
            Self::LowerBack => "Lower Back",
            Self::Quads => "Quadriceps",
            Self::Hamstrings => "Hamstrings",
            Self::Glutes => "Glutes",
            Self::Calves => "Calves",
            Self::FullBody => "Full Body",
        }
    }
}

/// Equipment types for exercises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Equipment {
    Barbell,
    Dumbbell,
    Kettlebell,
    Machine,
    Cable,
    Bodyweight,
    ResistanceBand,
    Other,
}

impl Equipment {
    /// Get all equipment types.
    pub fn all() -> &'static [Equipment] {
        &[
            Self::Barbell,
            Self::Dumbbell,
            Self::Kettlebell,
            Self::Machine,
            Self::Cable,
            Self::Bodyweight,
            Self::ResistanceBand,
            Self::Other,
        ]
    }

    /// Get display name for the equipment.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Barbell => "Barbell",
            Self::Dumbbell => "Dumbbell",
            Self::Kettlebell => "Kettlebell",
            Self::Machine => "Machine",
            Self::Cable => "Cable",
            Self::Bodyweight => "Bodyweight",
            Self::ResistanceBand => "Resistance Band",
            Self::Other => "Other",
        }
    }
}

/// Type of set in a workout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum SetType {
    /// Standard working set
    #[default]
    Working,
    /// Warm-up set (lighter weight)
    Warmup,
    /// Drop set (reduced weight after failure)
    Drop,
    /// Failure set (to muscle failure)
    Failure,
}

impl SetType {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Working => "Working",
            Self::Warmup => "Warm-up",
            Self::Drop => "Drop",
            Self::Failure => "Failure",
        }
    }
}

/// Status of a workout session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Session not yet started
    #[default]
    NotStarted,
    /// Session in progress
    InProgress,
    /// Session paused
    Paused,
    /// Session completed
    Completed,
    /// Session abandoned/cancelled
    Abandoned,
}

impl SessionStatus {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::NotStarted => "Not Started",
            Self::InProgress => "In Progress",
            Self::Paused => "Paused",
            Self::Completed => "Completed",
            Self::Abandoned => "Abandoned",
        }
    }
}

// ============================================================================
// Domain Entities
// ============================================================================

/// Exercise definition (library of exercises).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Exercise {
    /// Unique identifier.
    pub id: ExerciseId,
    /// Exercise name.
    pub name: String,
    /// Optional description/instructions.
    pub description: Option<String>,
    /// Primary muscle group targeted.
    pub primary_muscle: MuscleGroup,
    /// Secondary muscle groups.
    pub secondary_muscles: Vec<MuscleGroup>,
    /// Equipment required.
    pub equipment: Equipment,
    /// Whether this is a custom user exercise or built-in.
    pub is_custom: bool,
    /// Owner ID (for custom exercises).
    pub owner_id: Option<crate::UserId>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

impl Exercise {
    /// Create a new custom exercise.
    pub fn new_custom(
        id: ExerciseId,
        owner_id: crate::UserId,
        name: String,
        primary_muscle: MuscleGroup,
        equipment: Equipment,
    ) -> Self {
        Self {
            id,
            name,
            description: None,
            primary_muscle,
            secondary_muscles: vec![],
            equipment,
            is_custom: true,
            owner_id: Some(owner_id),
            created_at: Utc::now(),
        }
    }

    /// Create a built-in exercise.
    pub fn new_builtin(
        id: ExerciseId,
        name: String,
        description: Option<String>,
        primary_muscle: MuscleGroup,
        secondary_muscles: Vec<MuscleGroup>,
        equipment: Equipment,
    ) -> Self {
        Self {
            id,
            name,
            description,
            primary_muscle,
            secondary_muscles,
            equipment,
            is_custom: false,
            owner_id: None,
            created_at: Utc::now(),
        }
    }
}

/// Exercise within a template (with default sets/reps).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct TemplateExercise {
    /// Exercise ID.
    pub exercise_id: ExerciseId,
    /// Order in the template.
    pub order: u16,
    /// Target number of sets.
    pub target_sets: u8,
    /// Target reps per set (can be range like "8-12").
    pub target_reps: String,
    /// Optional rest time between sets (seconds).
    pub rest_seconds: Option<u16>,
    /// Optional notes for this exercise in the template.
    pub notes: Option<String>,
}

/// Workout template (reusable workout routine).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Template {
    /// Unique identifier.
    pub id: TemplateId,
    /// Owner of the template.
    pub owner_id: crate::UserId,
    /// Template name.
    pub name: String,
    /// Optional description.
    pub description: Option<String>,
    /// Exercises in the template.
    pub exercises: Vec<TemplateExercise>,
    /// Estimated duration in minutes.
    pub estimated_minutes: Option<u16>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

impl Template {
    /// Create a new empty template.
    pub fn new(id: TemplateId, owner_id: crate::UserId, name: String) -> Self {
        let now = Utc::now();
        Self {
            id,
            owner_id,
            name,
            description: None,
            exercises: vec![],
            estimated_minutes: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Add an exercise to the template.
    pub fn add_exercise(&mut self, exercise: TemplateExercise) {
        self.exercises.push(exercise);
        self.updated_at = Utc::now();
    }
}

/// A single set performed during a workout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct WorkoutSet {
    /// Unique identifier.
    pub id: SetId,
    /// Exercise this set is for.
    pub exercise_id: ExerciseId,
    /// Set number (1-indexed).
    pub set_number: u8,
    /// Type of set.
    pub set_type: SetType,
    /// Weight used (in user's preferred unit).
    pub weight: Option<f32>,
    /// Reps completed.
    pub reps: Option<u16>,
    /// Duration in seconds (for timed exercises).
    pub duration_seconds: Option<u16>,
    /// Distance in meters (for cardio).
    pub distance_meters: Option<f32>,
    /// Whether the set is completed.
    pub completed: bool,
    /// Completion timestamp.
    pub completed_at: Option<DateTime<Utc>>,
    /// Optional notes.
    pub notes: Option<String>,
}

impl WorkoutSet {
    /// Create a new pending set.
    pub fn new(id: SetId, exercise_id: ExerciseId, set_number: u8, set_type: SetType) -> Self {
        Self {
            id,
            exercise_id,
            set_number,
            set_type,
            weight: None,
            reps: None,
            duration_seconds: None,
            distance_meters: None,
            completed: false,
            completed_at: None,
            notes: None,
        }
    }

    /// Complete the set with weight and reps.
    pub fn complete_with_weight_reps(&mut self, weight: f32, reps: u16) {
        self.weight = Some(weight);
        self.reps = Some(reps);
        self.completed = true;
        self.completed_at = Some(Utc::now());
    }

    /// Complete the set with duration.
    pub fn complete_with_duration(&mut self, duration_seconds: u16) {
        self.duration_seconds = Some(duration_seconds);
        self.completed = true;
        self.completed_at = Some(Utc::now());
    }
}

/// A workout session (an instance of doing a workout).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct WorkoutSession {
    /// Unique identifier.
    pub id: SessionId,
    /// Owner of the session.
    pub owner_id: crate::UserId,
    /// Optional template this session is based on.
    pub template_id: Option<TemplateId>,
    /// Session name.
    pub name: String,
    /// Current status.
    pub status: SessionStatus,
    /// Sets in this session.
    pub sets: Vec<WorkoutSet>,
    /// Session start time.
    pub started_at: Option<DateTime<Utc>>,
    /// Session end time.
    pub ended_at: Option<DateTime<Utc>>,
    /// Optional notes.
    pub notes: Option<String>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

impl WorkoutSession {
    /// Create a new session.
    pub fn new(
        id: SessionId,
        owner_id: crate::UserId,
        name: String,
        template_id: Option<TemplateId>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id,
            owner_id,
            template_id,
            name,
            status: SessionStatus::NotStarted,
            sets: vec![],
            started_at: None,
            ended_at: None,
            notes: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Start the session.
    pub fn start(&mut self) {
        self.status = SessionStatus::InProgress;
        self.started_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    /// Pause the session.
    pub fn pause(&mut self) {
        self.status = SessionStatus::Paused;
        self.updated_at = Utc::now();
    }

    /// Resume the session.
    pub fn resume(&mut self) {
        self.status = SessionStatus::InProgress;
        self.updated_at = Utc::now();
    }

    /// Complete the session.
    pub fn complete(&mut self) {
        self.status = SessionStatus::Completed;
        self.ended_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    /// Abandon the session.
    pub fn abandon(&mut self) {
        self.status = SessionStatus::Abandoned;
        self.ended_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    /// Add a set to the session.
    pub fn add_set(&mut self, set: WorkoutSet) {
        self.sets.push(set);
        self.updated_at = Utc::now();
    }

    /// Calculate total volume (weight * reps) for the session.
    pub fn total_volume(&self) -> f32 {
        self.sets
            .iter()
            .filter(|s| s.completed)
            .filter_map(|s| match (s.weight, s.reps) {
                (Some(w), Some(r)) => Some(w * r as f32),
                _ => None,
            })
            .sum()
    }

    /// Get duration in seconds (if started).
    pub fn duration_seconds(&self) -> Option<i64> {
        match (self.started_at, self.ended_at) {
            (Some(start), Some(end)) => Some((end - start).num_seconds()),
            (Some(start), None) => Some((Utc::now() - start).num_seconds()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UserId;

    fn test_owner_id() -> UserId {
        UserId::from_uuid(Uuid::parse_str("550e8400-e29b-41d4-a716-446655440099").unwrap())
    }

    #[test]
    fn test_exercise_id_serialization() {
        let id = ExerciseId::new();
        let json = serde_json::to_string(&id).unwrap();
        let deserialized: ExerciseId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, deserialized);
    }

    #[test]
    fn test_muscle_group_display() {
        assert_eq!(MuscleGroup::LowerBack.display_name(), "Lower Back");
        assert_eq!(MuscleGroup::Quads.display_name(), "Quadriceps");
    }

    #[test]
    fn test_workout_session_lifecycle() {
        let owner = test_owner_id();
        let mut session =
            WorkoutSession::new(SessionId::new(), owner, "Test Workout".to_string(), None);

        assert_eq!(session.status, SessionStatus::NotStarted);
        assert!(session.started_at.is_none());

        session.start();
        assert_eq!(session.status, SessionStatus::InProgress);
        assert!(session.started_at.is_some());

        session.pause();
        assert_eq!(session.status, SessionStatus::Paused);

        session.resume();
        assert_eq!(session.status, SessionStatus::InProgress);

        session.complete();
        assert_eq!(session.status, SessionStatus::Completed);
        assert!(session.ended_at.is_some());
    }

    #[test]
    fn test_workout_set_completion() {
        let mut set = WorkoutSet::new(SetId::new(), ExerciseId::new(), 1, SetType::Working);

        assert!(!set.completed);
        assert!(set.weight.is_none());

        set.complete_with_weight_reps(100.0, 10);

        assert!(set.completed);
        assert_eq!(set.weight, Some(100.0));
        assert_eq!(set.reps, Some(10));
        assert!(set.completed_at.is_some());
    }

    #[test]
    fn test_total_volume_calculation() {
        let owner = test_owner_id();
        let mut session =
            WorkoutSession::new(SessionId::new(), owner, "Volume Test".to_string(), None);

        let exercise_id = ExerciseId::new();

        // Add completed sets
        let mut set1 = WorkoutSet::new(SetId::new(), exercise_id.clone(), 1, SetType::Working);
        set1.complete_with_weight_reps(100.0, 10); // 1000
        session.add_set(set1);

        let mut set2 = WorkoutSet::new(SetId::new(), exercise_id.clone(), 2, SetType::Working);
        set2.complete_with_weight_reps(100.0, 8); // 800
        session.add_set(set2);

        // Add incomplete set (should not count)
        let set3 = WorkoutSet::new(SetId::new(), exercise_id, 3, SetType::Working);
        session.add_set(set3);

        assert_eq!(session.total_volume(), 1800.0);
    }

    #[test]
    fn id_roundtrips_cover_all_frontend_workout_identifiers() {
        let raw = "550e8400-e29b-41d4-a716-446655440077";

        let exercise_id = ExerciseId::from_string(raw).unwrap();
        assert_eq!(exercise_id.as_str(), raw);
        assert_eq!(exercise_id.to_string(), raw);
        assert_eq!(exercise_id.as_uuid().to_string(), raw);

        let template_id = TemplateId::from_string(raw).unwrap();
        assert_eq!(template_id.as_str(), raw);
        assert_eq!(template_id.to_string(), raw);
        assert_eq!(template_id.as_uuid().to_string(), raw);

        let session_id = SessionId::from_string(raw).unwrap();
        assert_eq!(session_id.as_str(), raw);
        assert_eq!(session_id.to_string(), raw);
        assert_eq!(session_id.as_uuid().to_string(), raw);

        let set_id = SetId::from_string(raw).unwrap();
        assert_eq!(set_id.as_str(), raw);
        assert_eq!(set_id.to_string(), raw);
        assert_eq!(set_id.as_uuid().to_string(), raw);
    }

    #[test]
    fn enum_catalogs_cover_all_display_names() {
        assert_eq!(MuscleGroup::all().len(), 14);
        for muscle in MuscleGroup::all() {
            assert!(!muscle.display_name().is_empty());
        }

        assert_eq!(Equipment::all().len(), 8);
        for equipment in Equipment::all() {
            assert!(!equipment.display_name().is_empty());
        }

        assert_eq!(SetType::Working.display_name(), "Working");
        assert_eq!(SetType::Failure.display_name(), "Failure");
        assert_eq!(SessionStatus::NotStarted.display_name(), "Not Started");
        assert_eq!(SessionStatus::Abandoned.display_name(), "Abandoned");
    }

    #[test]
    fn exercise_template_and_session_builders_cover_custom_and_builtin_paths() {
        let owner = test_owner_id();
        let custom = Exercise::new_custom(
            ExerciseId::new(),
            owner,
            "Custom Dip".to_string(),
            MuscleGroup::Chest,
            Equipment::Bodyweight,
        );
        assert!(custom.is_custom);
        assert_eq!(custom.owner_id, Some(owner));
        assert_eq!(custom.equipment, Equipment::Bodyweight);

        let builtin = Exercise::new_builtin(
            ExerciseId::new(),
            "Lat Pulldown".to_string(),
            Some("Cable vertical pull".to_string()),
            MuscleGroup::Back,
            vec![MuscleGroup::Biceps],
            Equipment::Cable,
        );
        assert!(!builtin.is_custom);
        assert_eq!(builtin.owner_id, None);
        assert_eq!(builtin.secondary_muscles, vec![MuscleGroup::Biceps]);

        let mut template = Template::new(TemplateId::new(), owner, "Upper A".to_string());
        template.add_exercise(TemplateExercise {
            exercise_id: builtin.id.clone(),
            order: 0,
            target_sets: 4,
            target_reps: "8-10".to_string(),
            rest_seconds: Some(120),
            notes: Some("Full stretch".to_string()),
        });

        assert_eq!(template.name, "Upper A");
        assert_eq!(template.exercises.len(), 1);
        assert_eq!(template.exercises[0].target_sets, 4);
    }

    #[test]
    fn workout_set_and_session_duration_cover_timed_and_abandoned_paths() {
        let owner = test_owner_id();
        let exercise_id = ExerciseId::new();
        let mut timed = WorkoutSet::new(SetId::new(), exercise_id.clone(), 1, SetType::Warmup);
        timed.complete_with_duration(45);
        assert!(timed.completed);
        assert_eq!(timed.duration_seconds, Some(45));
        assert!(timed.completed_at.is_some());

        let mut session =
            WorkoutSession::new(SessionId::new(), owner, "Conditioning".to_string(), None);
        assert_eq!(session.duration_seconds(), None);

        session.start();
        session.started_at = Some(chrono::Utc::now() - chrono::Duration::seconds(180));
        session.add_set(timed);
        assert!(session.duration_seconds().unwrap() >= 179);

        session.abandon();
        assert_eq!(session.status, SessionStatus::Abandoned);
        assert!(session.ended_at.is_some());
        assert!(session.duration_seconds().unwrap() >= 0);
    }

    #[test]
    fn identifier_defaults_and_string_parsing_cover_success_and_failure_cases() {
        let raw = "550e8400-e29b-41d4-a716-446655440010";
        let uuid = Uuid::parse_str(raw).unwrap();

        assert_eq!(
            ExerciseId::from(raw.parse::<Uuid>().unwrap()).as_uuid(),
            &uuid
        );
        assert_eq!(TemplateId::from_uuid(uuid).to_string(), raw);
        assert_eq!(SessionId::from_string(raw).unwrap().to_string(), raw);
        assert_eq!(SetId::from_string(raw).unwrap().to_string(), raw);
        assert!(ExerciseId::from_string("bad-id").is_err());
        assert!("bad-id".parse::<TemplateId>().is_err());
        assert!("bad-id".parse::<SessionId>().is_err());
        assert!("bad-id".parse::<SetId>().is_err());

        let _ = ExerciseId::default();
        let _ = TemplateId::default();
        let _ = SessionId::default();
        let _ = SetId::default();
    }

    #[test]
    fn enum_catalogs_and_template_defaults_cover_remaining_branches() {
        let muscle_names: Vec<_> = MuscleGroup::all()
            .iter()
            .map(MuscleGroup::display_name)
            .collect();
        assert!(muscle_names.contains(&"Chest"));
        assert!(muscle_names.contains(&"Full Body"));

        let equipment_names: Vec<_> = Equipment::all()
            .iter()
            .map(Equipment::display_name)
            .collect();
        assert!(equipment_names.contains(&"Machine"));
        assert!(equipment_names.contains(&"Resistance Band"));

        assert_eq!(SetType::Warmup.display_name(), "Warm-up");
        assert_eq!(SetType::Drop.display_name(), "Drop");
        assert_eq!(SessionStatus::InProgress.display_name(), "In Progress");
        assert_eq!(SessionStatus::Completed.display_name(), "Completed");

        let owner = test_owner_id();
        let mut template = Template::new(TemplateId::new(), owner, "Push".to_string());
        assert!(template.description.is_none());
        assert!(template.estimated_minutes.is_none());
        assert!(template.updated_at >= template.created_at);

        let previous_updated_at = template.updated_at;
        template.add_exercise(TemplateExercise {
            exercise_id: ExerciseId::new(),
            order: 1,
            target_sets: 3,
            target_reps: "10".to_string(),
            rest_seconds: None,
            notes: None,
        });
        assert_eq!(template.exercises.len(), 1);
        assert!(template.updated_at >= previous_updated_at);
    }
}
