use crate::client::HttpClient;
use crate::error::ApiError;
use crate::types::*;

/// Auth endpoints for grok.com CLI code exchange and related flows.
impl HttpClient {
    /// Exchange a CLI authorization code for SSO tokens.
    /// POST /auth/exchange-grok-cli-code/
    pub async fn web_exchange_cli_code(
        &self,
        code: &str,
    ) -> Result<AuthExchangeResponse, ApiError> {
        let url = format!(
            "{}/auth/exchange-grok-cli-code/",
            self.config().web_base_url
        );
        let req = AuthExchangeRequest {
            code: code.to_string(),
        };
        let resp = self.post_json(&url, &req).await?;
        self.json(resp).await
    }
}
