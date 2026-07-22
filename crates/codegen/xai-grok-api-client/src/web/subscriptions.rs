use crate::client::HttpClient;
use crate::error::ApiError;
use crate::types::*;

/// Subscription info endpoints.
impl HttpClient {
    /// Get current subscription details.
    /// GET /rest/subscriptions
    pub async fn web_get_subscription(&self) -> Result<SubscriptionInfo, ApiError> {
        let url = format!("{}/rest/subscriptions", self.config().web_base_url);
        let resp = self.get(&url).await?;
        let wrapped: ApiResponse<SubscriptionInfo> = self.json(resp).await?;
        Ok(wrapped.result)
    }
}
