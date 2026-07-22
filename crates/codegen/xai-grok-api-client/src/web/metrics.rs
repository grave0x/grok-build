use crate::client::HttpClient;
use crate::error::ApiError;
use crate::types::*;

/// Metrics/logging endpoints.
impl HttpClient {
    /// Log a metric event.
    /// POST /api/log_metric
    pub async fn web_log_metric(&self, event: MetricEvent) -> Result<(), ApiError> {
        let url = format!("{}/api/log_metric", self.config().web_base_url);
        let resp = self.post_json(&url, &event).await?;
        let _: serde_json::Value = self.json(resp).await?;
        Ok(())
    }
}
