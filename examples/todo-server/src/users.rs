//! The cross-service reference check.
//!
//! # This is a fixture, not an architecture recommendation
//!
//! A real service split would not make a synchronous cross-service call to validate a foreign key.
//! It would accept the write and reconcile, or own the reference, or not have the reference. This
//! server does the naive thing on purpose, and `wiki/plans/d4d-multi-domain-trial.plan.md` says so
//! in the same words, because it is **the only thing that makes cross-service ordering
//! observable**: without a check, a todo that arrives before its user succeeds anyway, and the
//! trial's ordering test would prove routing rather than order.
//!
//! It is also why the trial's sabotage run works. Give each service its own scope, lose the shared
//! `seq`, and a todo can reach this server before the user record reaches the other one. Without
//! the check, nothing notices.

/// Asks the user server whether a user record exists.
///
/// Disabled when no base URL is configured, which is how every test written before D4d keeps
/// working: those trials have one server and no user domain, so there is nothing to check against
/// and a check that invented an answer would be worse than none.
#[derive(Clone)]
pub struct UserDirectory {
    client: reqwest::Client,
    base_url: Option<String>,
}

/// Why a reference check did not confirm the user.
#[derive(Debug)]
pub enum Missing {
    /// The user server answered, and it has no such user.
    NoSuchUser,
    /// The user server could not be reached, or answered something unusable.
    ///
    /// **Kept distinct from [`NoSuchUser`](Missing::NoSuchUser) deliberately.** "I know there is no
    /// such user" and "I could not find out" are different facts, and collapsing them would let a
    /// user-server outage present to the client as a durable ordering failure.
    Unavailable(String),
}

impl UserDirectory {
    /// A directory that checks against `base_url`, or checks nothing when it is `None`.
    pub fn new(base_url: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url,
        }
    }

    /// Whether checking is switched on at all.
    pub fn is_enabled(&self) -> bool {
        self.base_url.is_some()
    }

    /// Confirm a user record exists.
    ///
    /// # Errors
    ///
    /// The user is absent, or the user server could not answer.
    pub async fn require(&self, user_id: &str) -> Result<(), Missing> {
        let Some(base_url) = &self.base_url else {
            return Ok(());
        };
        let response = self
            .client
            .get(format!("{base_url}/api/v1/users/{user_id}"))
            .send()
            .await
            .map_err(|e| Missing::Unavailable(e.to_string()))?;

        match response.status() {
            status if status.is_success() => Ok(()),
            reqwest::StatusCode::NOT_FOUND => Err(Missing::NoSuchUser),
            other => Err(Missing::Unavailable(format!(
                "user server answered {other}"
            ))),
        }
    }
}
