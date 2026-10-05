//! External I/O boundary for the AWS Events API. Everything else talks to `EventsApi`.

pub mod http;
pub mod ops;
pub mod quota;
pub mod types;

use async_trait::async_trait;
pub use quota::Op;
pub use types::*;

/// Classified API failure. `Unknown` means "the write may or may not have been applied".
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ApiError {
    #[error("not signed in or token rejected (401)")]
    Unauthorized,
    #[error("forbidden (403): {0}")]
    Forbidden(String),
    #[error("not found (404)")]
    NotFound,
    #[error("operation is closed (409)")]
    Closed,
    #[error("throttled (429), retry after {retry_after_secs}s")]
    Throttled { retry_after_secs: u64 },
    #[error("bad request (400): {0}")]
    BadRequest(String),
    #[error("outcome unknown: {0}")]
    Unknown(String),
}

impl From<ApiError> for crate::error::RivError {
    fn from(e: ApiError) -> Self {
        match e {
            ApiError::Unauthorized | ApiError::Forbidden(_) => crate::error::RivError::auth(e),
            other => crate::error::RivError::general(other),
        }
    }
}

pub type ApiResult<T> = std::result::Result<T, ApiError>;

/// Real, mock-backed, and recording implementations all satisfy this trait.
#[async_trait]
pub trait EventsApi: Send + Sync {
    async fn list_events(&self) -> ApiResult<Vec<Event>>;
    async fn get_event(&self, event_id: &str) -> ApiResult<Event>;
    async fn list_sessions_page(&self, event_id: &str, p: &ListSessionsParams) -> ApiResult<SessionPage>;
    async fn get_session(&self, event_id: &str, session_id: &str, locale: Option<&str>) -> ApiResult<Session>;
    async fn get_schedule(&self, event_id: &str) -> ApiResult<Schedule>;
    async fn reserve(&self, event_id: &str, ids: &[String]) -> ApiResult<BulkResult>;
    async fn cancel_reservation(&self, event_id: &str, session_id: &str) -> ApiResult<()>;
    async fn associate_favorites(&self, event_id: &str, ids: &[String]) -> ApiResult<BulkResult>;
    async fn disassociate_favorite(&self, event_id: &str, session_id: &str) -> ApiResult<()>;
    async fn create_personal_time(&self, event_id: &str, input: &PersonalTimeInput) -> ApiResult<()>;
    async fn update_personal_time(&self, event_id: &str, id: &str, input: &PersonalTimeInput) -> ApiResult<()>;
    async fn delete_personal_time(&self, event_id: &str, id: &str) -> ApiResult<()>;

    /// Locally tracked remaining quota for `op` (units: requests, or sessions for bulk ops).
    fn remaining(&self, _op: Op) -> u32 {
        u32::MAX
    }
    /// Sleep until quota frees up for `op`. No-op for implementations without limits.
    async fn wait_for_quota(&self, _op: Op) {}
}
