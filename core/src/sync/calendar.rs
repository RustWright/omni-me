//! A fresh calendar read for a device that holds no source: the server reads, the device pulls.

use async_trait::async_trait;
use reqwest::StatusCode;

use super::client::{SyncClient, SyncError};
use super::puller::PullScheduler;
use crate::calendar::sync::Refresh;

/// [`Refresh`] by asking the server. A server with no calendar source, or one built before the
/// route existed, answers 404, which is not a failure: there is nothing fresher to read.
pub struct ServerCalendar {
    pub client: SyncClient,
    pub pulls: PullScheduler,
}

#[async_trait]
impl Refresh for ServerCalendar {
    async fn refresh(&self) -> Result<(), String> {
        let read = self.client.refresh_calendar().await;
        // Pulled either way: a failed read leaves a status event to bring down.
        self.pulls.pull_now().await;
        read.map(|_| ()).map_err(|e| e.to_string())
    }
}

impl SyncClient {
    /// `POST /calendar/refresh`. `Ok(false)` when the server has nothing to read.
    pub async fn refresh_calendar(&self) -> Result<bool, SyncError> {
        let resp = self
            .http()
            .post(format!("{}/calendar/refresh", self.server_url()))
            .send()
            .await
            .map_err(|e| SyncError::Network(e.to_string()))?;
        match resp.status() {
            s if s.is_success() => Ok(true),
            StatusCode::NOT_FOUND => Ok(false),
            s => {
                let body = resp.text().await.unwrap_or_default();
                Err(SyncError::Server(format!(
                    "calendar refresh failed ({s}): {body}"
                )))
            }
        }
    }
}
