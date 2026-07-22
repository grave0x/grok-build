use async_trait::async_trait;

/// Authentication strategy for API requests.
#[async_trait]
pub trait ApiAuth: Send + Sync {
    /// Apply auth headers to a request builder.
    async fn apply(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder;
    /// Attempt to refresh credentials. Returns true if successfully refreshed.
    async fn refresh(&self) -> Result<bool, crate::ApiError> {
        let _ = self;
        Ok(false)
    }
}

/// Bearer token authentication (used by Grok Build API).
#[derive(Clone, Debug)]
pub struct BearerAuth {
    token: String,
}

impl BearerAuth {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
        }
    }

    pub fn token(&self) -> &str {
        &self.token
    }
}

#[async_trait]
impl ApiAuth for BearerAuth {
    async fn apply(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        builder.bearer_auth(&self.token)
    }
}

/// SSO cookie-based authentication (used by grok.com web API).
/// Sends `sso`, `sso-rw`, and `x-userid` cookies with requests.
#[derive(Clone, Debug)]
pub struct SsoAuth {
    sso: String,
    sso_rw: Option<String>,
    x_userid: Option<String>,
    csrf_token: Option<String>,
}

impl SsoAuth {
    pub fn new(sso: impl Into<String>) -> Self {
        Self {
            sso: sso.into(),
            sso_rw: None,
            x_userid: None,
            csrf_token: None,
        }
    }

    pub fn with_sso_rw(mut self, token: impl Into<String>) -> Self {
        self.sso_rw = Some(token.into());
        self
    }

    pub fn with_x_userid(mut self, id: impl Into<String>) -> Self {
        self.x_userid = Some(id.into());
        self
    }

    pub fn with_csrf(mut self, token: impl Into<String>) -> Self {
        self.csrf_token = Some(token.into());
        self
    }
}

#[async_trait]
impl ApiAuth for SsoAuth {
    async fn apply(&self, mut builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let mut parts = Vec::new();
        parts.push(format!("sso={}", &self.sso));
        if let Some(rw) = &self.sso_rw {
            parts.push(format!("sso-rw={}", rw));
        }
        if let Some(uid) = &self.x_userid {
            parts.push(format!("x-userid={}", uid));
        }
        if let Some(csrf) = &self.csrf_token {
            builder = builder.header("x-csrf-token", csrf);
        }
        let cookie_value = parts.join("; ");
        builder.header(reqwest::header::COOKIE, cookie_value)
    }
}

/// No authentication (for public endpoints).
#[derive(Clone, Debug)]
pub struct NoAuth;

#[async_trait]
impl ApiAuth for NoAuth {
    async fn apply(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        builder
    }
}
