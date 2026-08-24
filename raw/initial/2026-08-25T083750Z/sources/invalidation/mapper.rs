use shared::OutcomeStatus;

use crate::server::EntityType;

/// Map a mutation target service + outcome status into cache entities that must be invalidated.
pub fn invalidation_entities_for_outcome(
    service: &str,
    status: OutcomeStatus,
) -> &'static [EntityType] {
    match status {
        // Do not bump versions on duplicates unless we can prove state changed.
        OutcomeStatus::Duplicate => &[],
        OutcomeStatus::Applied => match service {
            "user-api" => &[EntityType::UserPreferences],
            "exercise-api" => &[EntityType::Exercises],
            "workout-api" => &[EntityType::Sessions, EntityType::Templates],
            _ => &[],
        },
        OutcomeStatus::Rejected | OutcomeStatus::Blocked | OutcomeStatus::Failed => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapped_entities_are_scoped_by_service_and_status() {
        assert_eq!(
            invalidation_entities_for_outcome("user-api", OutcomeStatus::Applied),
            &[EntityType::UserPreferences]
        );
        assert_eq!(
            invalidation_entities_for_outcome("exercise-api", OutcomeStatus::Applied),
            &[EntityType::Exercises]
        );
        assert_eq!(
            invalidation_entities_for_outcome("workout-api", OutcomeStatus::Applied),
            &[EntityType::Sessions, EntityType::Templates]
        );
        assert!(invalidation_entities_for_outcome("user-api", OutcomeStatus::Duplicate).is_empty());
    }
}
