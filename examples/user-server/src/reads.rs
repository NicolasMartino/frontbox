//! What a client reads, and the version that tells it when to read again.
//!
//! Split out of `lib.rs` when the delete cascade pushed it past the four-hundred-line cap
//! `AGENTS.md` sets and `scripts/verify.sh` measures — the same line, drawn the same way, as
//! `examples/todo-server/src/reads.rs`. Nothing here writes, and the version endpoint exists only
//! to describe what these reads would return.

use std::sync::atomic::Ordering;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;

use crate::wire::{User, Versions};
use crate::AppState;

/// Every user. This is what makes another user's todos attributable.
#[utoipa::path(
    get,
    path = "/api/v1/users",
    tag = "users",
    responses(
        (status = 200, description = "Current user read model.", body = [User]),
        (status = 500, description = "Database failure."),
        (status = 503, description = "This service is down.")
    )
)]
pub(crate) async fn list(State(state): State<AppState>) -> Result<Json<Vec<User>>, StatusCode> {
    Ok(Json(read_users(&state).await?))
}

/// One user, for the todo server's reference check.
#[utoipa::path(
    get,
    path = "/api/v1/users/{id}",
    tag = "users",
    params(("id" = String, Path, description = "User row id.")),
    responses(
        (status = 200, description = "The user.", body = User),
        (status = 404, description = "No such user."),
        (status = 500, description = "Database failure."),
        (status = 503, description = "This service is down.")
    )
)]
pub(crate) async fn one(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<User>, StatusCode> {
    refuse_if_down(&state)?;
    let row: Option<(String, String)> = sqlx::query_as("SELECT id, name FROM users WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    // The 404 that makes cross-service ordering observable. `todo-server` turns it into a refusal
    // to accept the todo, and the trial's transport turns *that* into `Blocked` rather than
    // `Rejected` — a missing prerequisite is transient (`wiki/decisions/019-verdict-synthesis.decision.md`).
    let (id, name) = row.ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(User { id, name }))
}

/// The opaque version of every entity this server owns.
#[utoipa::path(
    get,
    path = "/api/v1/versions",
    tag = "invalidation",
    responses(
        (status = 200, description = "Opaque per-entity versions.", body = Versions),
        (status = 500, description = "Database failure."),
        (status = 503, description = "This service is down.")
    )
)]
pub(crate) async fn versions(State(state): State<AppState>) -> Result<Json<Versions>, StatusCode> {
    let users = read_users(&state).await?;
    Ok(Json(Versions {
        user: version_of(users.iter().map(|u| (u.id.as_str(), u.name.as_str()))),
    }))
}

/// The outage switch, in front of every read.
///
/// # Why this is a function and not a line
///
/// It used to be a line inside [`read_users`], which `list` and `versions` both go through and
/// [`one`] does not — `one` reads a single row with its own query. So the switch covered two of the
/// three reads while `AppState::down`'s own documentation promised all of them, and the read it
/// missed is the only one another *service* makes.
///
/// The consequence was not cosmetic. `todo-server` asks this endpoint whether a todo's owner
/// exists, and it distinguishes "there is no such user" from "I could not find out" precisely so an
/// outage cannot present to a client as a durable ordering failure
/// (`examples/todo-server/src/users.rs`). With this read exempt, the second branch was unreachable:
/// the fixture could not produce the condition its own error type exists to name.
fn refuse_if_down(state: &AppState) -> Result<(), StatusCode> {
    if state.down.load(Ordering::SeqCst) {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    Ok(())
}

async fn read_users(state: &AppState) -> Result<Vec<User>, StatusCode> {
    refuse_if_down(state)?;
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT id, name FROM users ORDER BY id")
        .fetch_all(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(rows
        .into_iter()
        .map(|(id, name)| User { id, name })
        .collect())
}

/// An opaque version derived from the state itself.
///
/// # Derived, not counted
///
/// A monotonic counter bumped on every write would be simpler and would be *wrong* in the one case
/// the trial cares about: a mutation that changes nothing — a rename to the same name, a replayed
/// `Duplicate` — would bump it and every client would refetch identical data. A version computed
/// from the rows changes exactly when the rows do.
///
/// # Opaque, and the client is held to that
///
/// FNV-1a rendered as hex, which is not ordered, not a timestamp, and not a row count. Decision 012
/// and the cache runtime compare versions for **equality** and never for order; a version a client
/// could subtract would invite the comparison the design forbids, and picking a value that
/// visibly cannot be subtracted is cheaper than documenting that it must not be.
///
/// Not a cryptographic digest: this defends against accidental staleness, not against a server
/// choosing to lie about its own state.
pub(crate) fn version_of<'a>(rows: impl Iterator<Item = (&'a str, &'a str)>) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut byte = |b: u8| {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    };
    for (id, name) in rows {
        // A separator that cannot occur in either field, so `("ab", "c")` and `("a", "bc")` are
        // different inputs rather than the same concatenation.
        for b in id.as_bytes() {
            byte(*b);
        }
        byte(0);
        for b in name.as_bytes() {
            byte(*b);
        }
        byte(0);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The property that makes this usable as an invalidation signal at all.
    #[test]
    fn a_version_tracks_the_state_and_nothing_else() {
        let empty = version_of(std::iter::empty());
        let one = version_of([("u1", "Alice")].into_iter());
        let renamed = version_of([("u1", "Alicia")].into_iter());
        let two = version_of([("u1", "Alice"), ("u2", "Bob")].into_iter());

        assert_ne!(empty, one, "adding a user changes the version");
        assert_ne!(one, renamed, "renaming a user changes the version");
        assert_ne!(one, two, "a second user changes the version");
        assert_eq!(
            one,
            version_of([("u1", "Alice")].into_iter()),
            "identical state is identical version — this is what stops a no-op write from \
             invalidating every client's cache"
        );
    }

    /// The separator earns itself: without it these two states hash alike.
    #[test]
    fn field_boundaries_are_part_of_the_input() {
        assert_ne!(
            version_of([("ab", "c")].into_iter()),
            version_of([("a", "bc")].into_iter())
        );
    }
}
