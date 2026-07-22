use std::sync::LazyLock;
use std::time::Duration;

use crate::auth::{ApiAuth, BearerAuth, SsoAuth};
use crate::error::ApiError;

static SHARED_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .connect_timeout(Duration::from_secs(30))
        .http2_keep_alive_interval(Some(Duration::from_secs(10)))
        .http2_keep_alive_timeout(Duration::from_secs(5))
        .http2_keep_alive_while_idle(true)
        .pool_idle_timeout(Duration::from_secs(60))
        .build()
        .expect("Failed to build HTTP client")
});

/// Shared configuration for all API clients.
#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub build_base_url: String,
    pub web_base_url: String,
    pub timeout: Duration,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            build_base_url: "https://cli-chat-proxy.grok.com".to_string(),
            web_base_url: "https://grok.com".to_string(),
            timeout: Duration::from_secs(120),
        }
    }
}

/// Low-level HTTP client that applies auth and sends requests.
#[derive(Clone)]
pub struct HttpClient {
    inner: reqwest::Client,
    config: ClientConfig,
    auth: Option<std::sync::Arc<dyn ApiAuth>>,
}

impl HttpClient {
    pub fn new(config: ClientConfig) -> Self {
        Self {
            inner: SHARED_CLIENT.clone(),
            config,
            auth: None,
        }
    }

    /// Create a client pre-configured with Bearer token auth.
    pub fn new_bearer(token: String, timeout_secs: u64) -> Self {
        Self::new(ClientConfig {
            timeout: Duration::from_secs(timeout_secs),
            ..Default::default()
        })
        .with_auth(BearerAuth::new(token))
    }

    /// Create a client pre-configured with SSO cookie auth.
    pub fn new_sso(cookie: String, timeout_secs: u64) -> Self {
        Self::new(ClientConfig {
            timeout: Duration::from_secs(timeout_secs),
            ..Default::default()
        })
        .with_auth(SsoAuth::new(cookie))
    }

    pub fn with_auth(mut self, auth: impl ApiAuth + 'static) -> Self {
        self.auth = Some(std::sync::Arc::new(auth));
        self
    }

    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    pub fn auth(&self) -> Option<&std::sync::Arc<dyn ApiAuth>> {
        self.auth.as_ref()
    }

    pub fn shared_client(&self) -> &reqwest::Client {
        &self.inner
    }

    /// Prepare a `GET` request with auth applied.
    pub async fn get(&self, url: &str) -> Result<reqwest::Response, ApiError> {
        let mut builder = self.inner.get(url);
        if let Some(auth) = &self.auth {
            builder = auth.apply(builder).await;
        }
        let resp = builder.send().await?;
        check_status_internal(&resp).await?;
        Ok(resp)
    }

    /// Prepare a `POST` request with auth applied and JSON body.
    pub async fn post_json<T: serde::Serialize + ?Sized>(
        &self,
        url: &str,
        body: &T,
    ) -> Result<reqwest::Response, ApiError> {
        let mut builder = self.inner.post(url).json(body);
        if let Some(auth) = &self.auth {
            builder = auth.apply(builder).await;
        }
        let resp = builder.send().await?;
        check_status_internal(&resp).await?;
        Ok(resp)
    }

    /// POST with raw bytes body (multipart, binary).
    pub async fn post_bytes(
        &self,
        url: &str,
        content_type: &str,
        body: bytes::Bytes,
    ) -> Result<reqwest::Response, ApiError> {
        let mut builder = self
            .inner
            .post(url)
            .header(reqwest::header::CONTENT_TYPE, content_type)
            .body(body);
        if let Some(auth) = &self.auth {
            builder = auth.apply(builder).await;
        }
        let resp = builder.send().await?;
        check_status_internal(&resp).await?;
        Ok(resp)
    }

    /// PUT with raw bytes.
    pub async fn put_bytes(
        &self,
        url: &str,
        content_type: &str,
        body: bytes::Bytes,
    ) -> Result<reqwest::Response, ApiError> {
        let mut builder = self
            .inner
            .put(url)
            .header(reqwest::header::CONTENT_TYPE, content_type)
            .body(body);
        if let Some(auth) = &self.auth {
            builder = auth.apply(builder).await;
        }
        let resp = builder.send().await?;
        check_status_internal(&resp).await?;
        Ok(resp)
    }

    /// DELETE request.
    pub async fn delete(&self, url: &str) -> Result<reqwest::Response, ApiError> {
        let mut builder = self.inner.delete(url);
        if let Some(auth) = &self.auth {
            builder = auth.apply(builder).await;
        }
        let resp = builder.send().await?;
        check_status_internal(&resp).await?;
        Ok(resp)
    }

    /// Parse response as JSON.
    pub async fn json<T: serde::de::DeserializeOwned>(&self, resp: reqwest::Response) -> Result<T, ApiError> {
        Ok(resp.json().await?)
    }

    /// Parse response as text.
    pub async fn text(&self, resp: reqwest::Response) -> Result<String, ApiError> {
        Ok(resp.text().await?)
    }

    /// Get the inner client for custom request building.
    pub fn inner(&self) -> &reqwest::Client {
        &self.inner
    }

    /// Build a request with auth applied (for streaming or custom requests).
    pub async fn request(
        &self,
        method: reqwest::Method,
        url: &str,
    ) -> reqwest::RequestBuilder {
        let mut builder = self.inner.request(method, url);
        if let Some(auth) = &self.auth {
            builder = auth.apply(builder).await;
        }
        builder
    }
}

pub(crate) async fn check_status_internal(resp: &reqwest::Response) -> Result<(), ApiError> {
    let status = resp.status().as_u16();
    if status >= 200 && status < 300 {
        return Ok(());
    }
    let reason = resp.status().canonical_reason().unwrap_or("Unknown").to_string();
    Err(ApiError::Http {
        status,
        message: reason,
    })
}
