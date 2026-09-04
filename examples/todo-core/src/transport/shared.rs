//! The shared transport handle used by draining and invalidation.

use frontbox::{Error, MutationBatchRequest, MutationBatchResponse, SyncTransport};

use super::HttpTransport;

/// An [`HttpTransport`] two owners share.
///
/// # Why a newtype rather than `Rc<HttpTransport>` directly
///
/// D4d gave this crate a second consumer of the same HTTP client: the invalidation sources poll
/// `GET /api/v1/versions` on both services, and they must see the same offline switch the drain
/// does — a trial where "offline" stops writes but not polls would be testing a client nobody
/// ships.
///
/// `SyncRunner` owns its transport, so sharing means `Rc`. Implementing `SyncTransport` for
/// `Rc<HttpTransport>` is not allowed here — both the trait and `Rc` are foreign to this crate —
/// so the newtype is the orphan rule's price, paid once, in five lines.
#[derive(Clone)]
pub struct SharedTransport(std::rc::Rc<HttpTransport>);

impl SharedTransport {
    /// Wrap a transport for sharing.
    pub fn new(transport: HttpTransport) -> Self {
        Self(std::rc::Rc::new(transport))
    }

    /// A second handle on the same transport.
    #[must_use]
    pub fn handle(&self) -> std::rc::Rc<HttpTransport> {
        std::rc::Rc::clone(&self.0)
    }
}

impl std::ops::Deref for SharedTransport {
    type Target = HttpTransport;

    fn deref(&self) -> &HttpTransport {
        &self.0
    }
}

impl SyncTransport for SharedTransport {
    async fn send_batch(
        &self,
        request: MutationBatchRequest,
    ) -> Result<MutationBatchResponse, Error> {
        self.0.send_batch(request).await
    }
}
