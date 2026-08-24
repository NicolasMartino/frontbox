//! Entity cache with version tracking for cache invalidation.
//!
//! Tracks version numbers per entity type on the client side.
//! SSE events update versions and mark entities as stale.
//! Self-correction: version anomalies trigger reset to 0.

use std::collections::{HashMap, HashSet};

#[cfg(feature = "server")]
use observability::{record_cache_hit, record_cache_invalidation, record_cache_miss};

/// Entity types that can be cached.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntityType {
    Exercise,
    Session,
    Template,
    UserPreferences,
}

impl EntityType {
    /// Get the string representation (matches server EntityType).
    pub fn as_str(&self) -> &'static str {
        match self {
            EntityType::Exercise => "exercises",
            EntityType::Session => "sessions",
            EntityType::Template => "templates",
            EntityType::UserPreferences => "user_preferences",
        }
    }

    /// Parse from string (SSE event entity name).
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "exercises" => Some(EntityType::Exercise),
            "sessions" => Some(EntityType::Session),
            "templates" => Some(EntityType::Template),
            "user_preferences" => Some(EntityType::UserPreferences),
            _ => None,
        }
    }

    /// All entity types.
    pub fn all() -> &'static [EntityType] {
        &[
            EntityType::Exercise,
            EntityType::Session,
            EntityType::Template,
            EntityType::UserPreferences,
        ]
    }
}

/// Result of version update - may require reset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionUpdateResult {
    /// Version updated normally (server ahead, marked stale).
    Updated,
    /// No change needed (versions match).
    NoChange,
    /// Anomaly detected - needs reset (server behind client).
    NeedsReset,
}

/// Client-side cache state with version tracking.
#[derive(Clone, Debug)]
pub struct EntityCache {
    /// Current known version per entity type.
    versions: HashMap<EntityType, u64>,
    /// Entities marked as stale (need refetch).
    stale: HashSet<EntityType>,
}

impl Default for EntityCache {
    fn default() -> Self {
        Self::new()
    }
}

impl EntityCache {
    /// Create a new empty entity cache.
    pub fn new() -> Self {
        Self {
            versions: HashMap::new(),
            stale: HashSet::new(),
        }
    }

    /// Update version from SSE event.
    /// Returns whether a reset is needed (server < client = anomaly).
    pub fn update_version(
        &mut self,
        entity: EntityType,
        server_version: Option<u64>,
    ) -> VersionUpdateResult {
        let server_version = server_version.unwrap_or(0);
        let current = self.versions.get(&entity).copied().unwrap_or(0);

        if server_version > current {
            // Normal: server ahead, update and mark stale
            self.versions.insert(entity, server_version);
            self.stale.insert(entity);

            // Record cache invalidation metric
            #[cfg(feature = "server")]
            record_cache_invalidation(entity.as_str());

            VersionUpdateResult::Updated
        } else if server_version < current {
            // Anomaly: server behind client - needs reset
            VersionUpdateResult::NeedsReset
        } else {
            // Equal: already in sync
            VersionUpdateResult::NoChange
        }
    }

    /// Reset local version to 0 and mark stale (after server reset).
    pub fn reset_version(&mut self, entity: EntityType) {
        self.versions.insert(entity, 0);
        self.stale.insert(entity);
    }

    /// Check versions against server, returns entities that need reset.
    pub fn check_versions(
        &mut self,
        server_versions: &HashMap<EntityType, u64>,
    ) -> Vec<EntityType> {
        let mut needs_reset = Vec::new();

        for (entity, server_version) in server_versions {
            if self.update_version(*entity, Some(*server_version))
                == VersionUpdateResult::NeedsReset
            {
                needs_reset.push(*entity);
            }
        }

        needs_reset
    }

    /// Check if entity is stale.
    pub fn is_stale(&self, entity: EntityType) -> bool {
        let is_stale = self.stale.contains(&entity);

        // Record cache hit/miss metrics
        #[cfg(feature = "server")]
        {
            if is_stale {
                record_cache_miss(entity.as_str());
            } else {
                record_cache_hit(entity.as_str());
            }
        }

        is_stale
    }

    /// Mark entity as fresh (after refetch).
    pub fn mark_fresh(&mut self, entity: EntityType) {
        self.stale.remove(&entity);
    }

    /// Get current versions for reconnect check.
    pub fn get_versions(&self) -> HashMap<String, u64> {
        self.versions
            .iter()
            .map(|(e, v)| (e.as_str().to_string(), *v))
            .collect()
    }

    /// Get version for a specific entity (for reactive updates).
    pub fn get_version(&self, entity: EntityType) -> u64 {
        self.versions.get(&entity).copied().unwrap_or(0)
    }

    /// Get all stale entities.
    pub fn stale_entities(&self) -> Vec<EntityType> {
        self.stale.iter().copied().collect()
    }

    /// Mark all entities as stale (used on error recovery).
    pub fn mark_all_stale(&mut self) {
        for entity in EntityType::all() {
            self.stale.insert(*entity);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_version_normal() {
        let mut cache = EntityCache::new();

        // First update sets version
        let result = cache.update_version(EntityType::Exercise, Some(5));
        assert_eq!(result, VersionUpdateResult::Updated);
        assert!(cache.is_stale(EntityType::Exercise));
        assert_eq!(cache.get_version(EntityType::Exercise), 5);
    }

    #[test]
    fn test_update_version_no_change() {
        let mut cache = EntityCache::new();
        cache.versions.insert(EntityType::Exercise, 5);

        let result = cache.update_version(EntityType::Exercise, Some(5));
        assert_eq!(result, VersionUpdateResult::NoChange);
        assert!(!cache.is_stale(EntityType::Exercise));
    }

    #[test]
    fn test_update_version_anomaly() {
        let mut cache = EntityCache::new();
        cache.versions.insert(EntityType::Exercise, 10);

        // Server behind client = anomaly
        let result = cache.update_version(EntityType::Exercise, Some(5));
        assert_eq!(result, VersionUpdateResult::NeedsReset);
    }

    #[test]
    fn test_reset_version() {
        let mut cache = EntityCache::new();
        cache.versions.insert(EntityType::Exercise, 10);

        cache.reset_version(EntityType::Exercise);
        assert_eq!(cache.get_version(EntityType::Exercise), 0);
        assert!(cache.is_stale(EntityType::Exercise));
    }

    #[test]
    fn test_mark_fresh() {
        let mut cache = EntityCache::new();
        cache.stale.insert(EntityType::Exercise);

        cache.mark_fresh(EntityType::Exercise);
        assert!(!cache.is_stale(EntityType::Exercise));
    }

    #[test]
    fn test_check_versions() {
        let mut cache = EntityCache::new();
        cache.versions.insert(EntityType::Exercise, 10); // Will need reset (server=5)
        cache.versions.insert(EntityType::Session, 3); // Will update (server=5)

        let mut server_versions = HashMap::new();
        server_versions.insert(EntityType::Exercise, 5); // Anomaly
        server_versions.insert(EntityType::Session, 5); // Update

        let needs_reset = cache.check_versions(&server_versions);
        assert_eq!(needs_reset, vec![EntityType::Exercise]);
        assert!(cache.is_stale(EntityType::Session));
    }
}
