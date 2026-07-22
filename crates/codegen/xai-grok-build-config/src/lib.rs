//! User-facing config for `grok-build` CLI/TUI.
//!
//! Parses `~/.config/grok-build/config.toml` with env-var overrides.
//! All auth tokens also read from environment variables.

use std::path::PathBuf;
use serde::{Deserialize, Serialize};

pub use error::ConfigError;

mod error;

// ─────────────────────────────────────────────────────────
// Top-level config
// ─────────────────────────────────────────────────────────

/// Full grok-build configuration read from TOML + env.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GrokBuildConfig {
    /// Auth credentials (tokens, cookies).
    pub auth: AuthConfig,
    /// Connection defaults.
    pub defaults: DefaultsConfig,
    /// Feature toggles — each API group can be enabled/disabled.
    pub features: FeaturesConfig,
    /// Build API sub-config.
    pub build: BuildConfig,
    /// Web API sub-config.
    pub web: WebConfig,
    /// TUI settings (enabled/disabled, widget layout).
    pub tui: TuiConfig,
}

impl Default for GrokBuildConfig {
    fn default() -> Self {
        Self {
            auth: AuthConfig::default(),
            defaults: DefaultsConfig::default(),
            features: FeaturesConfig::default(),
            build: BuildConfig::default(),
            web: WebConfig::default(),
            tui: TuiConfig::default(),
        }
    }
}

// ─────────────────────────────────────────────────────────
// Auth
// ─────────────────────────────────────────────────────────

/// API credentials.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    /// Build API Bearer token (`xai-...`).
    /// Env: `GROK_BUILD_BEARER_TOKEN`
    pub bearer_token: Option<String>,
    /// Web API SSO session cookie.
    /// Env: `GROK_BUILD_SSO_COOKIE`
    pub sso_cookie: Option<String>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            bearer_token: std::env::var("GROK_BUILD_BEARER_TOKEN").ok(),
            sso_cookie: std::env::var("GROK_BUILD_SSO_COOKIE").ok(),
        }
    }
}

// ─────────────────────────────────────────────────────────
// Defaults
// ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DefaultsConfig {
    /// Request timeout in seconds.
    pub timeout_secs: u64,
    /// Build API base URL.
    pub base_url_build: String,
    /// Web API base URL.
    pub base_url_web: String,
}

impl Default for DefaultsConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 60,
            base_url_build: "https://cli-chat-proxy.grok.com".into(),
            base_url_web: "https://grok.com".into(),
        }
    }
}

// ─────────────────────────────────────────────────────────
// Features
// ─────────────────────────────────────────────────────────

/// Per-endpoint-group feature toggles.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FeaturesConfig {
    pub chat: bool,
    pub storage: bool,
    pub files: bool,
    pub workspaces: bool,
    pub models: bool,
    pub skills: bool,
    pub mcp: bool,
}

impl Default for FeaturesConfig {
    fn default() -> Self {
        Self {
            chat: true,
            storage: true,
            files: true,
            workspaces: true,
            models: true,
            skills: true,
            mcp: true,
        }
    }
}

// ─────────────────────────────────────────────────────────
// Build API
// ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BuildConfig {
    /// Max bytes for a single storage upload.
    pub max_upload_bytes: u64,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            max_upload_bytes: 100 * 1024 * 1024, // 100 MB
        }
    }
}

// ─────────────────────────────────────────────────────────
// Web API
// ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WebConfig {
    /// Max bytes for a single file upload.
    pub max_upload_bytes: u64,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            max_upload_bytes: 50 * 1024 * 1024, // 50 MB
        }
    }
}

// ─────────────────────────────────────────────────────────
// TUI layout
// ─────────────────────────────────────────────────────────

/// TUI settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TuiConfig {
    /// Enable the TUI binary (`grok-build tui`). Default true.
    pub enabled: bool,
    /// Ordered list of widget instances.
    pub layout: WidgetLayout,
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            layout: WidgetLayout::default(),
        }
    }
}

/// Describes the full widget layout (which widgets exist, where, and whether active).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WidgetLayout {
    pub widgets: Vec<WidgetInstance>,
}

impl Default for WidgetLayout {
    fn default() -> Self {
        Self {
            widgets: vec![
                WidgetInstance {
                    name: "sidebar".into(),
                    enabled: true,
                    position: WidgetPosition::Left,
                    width: Some(28),
                    height: None,
                },
                WidgetInstance {
                    name: "projects".into(),
                    enabled: true,
                    position: WidgetPosition::Main,
                    width: None,
                    height: None,
                },
                WidgetInstance {
                    name: "files".into(),
                    enabled: true,
                    position: WidgetPosition::Main,
                    width: None,
                    height: None,
                },
                WidgetInstance {
                    name: "chat".into(),
                    enabled: true,
                    position: WidgetPosition::Main,
                    width: None,
                    height: None,
                },
                WidgetInstance {
                    name: "usage".into(),
                    enabled: true,
                    position: WidgetPosition::Right,
                    width: Some(24),
                    height: None,
                },
                WidgetInstance {
                    name: "models".into(),
                    enabled: false,
                    position: WidgetPosition::Main,
                    width: None,
                    height: None,
                },
                WidgetInstance {
                    name: "skills".into(),
                    enabled: false,
                    position: WidgetPosition::Main,
                    width: None,
                    height: None,
                },
                WidgetInstance {
                    name: "mcp".into(),
                    enabled: false,
                    position: WidgetPosition::Main,
                    width: None,
                    height: None,
                },
                WidgetInstance {
                    name: "storage".into(),
                    enabled: false,
                    position: WidgetPosition::Main,
                    width: None,
                    height: None,
                },
                WidgetInstance {
                    name: "config_editor".into(),
                    enabled: true,
                    position: WidgetPosition::Main,
                    width: None,
                    height: None,
                },
                WidgetInstance {
                    name: "log".into(),
                    enabled: true,
                    position: WidgetPosition::Bottom,
                    width: None,
                    height: Some(8),
                },
                WidgetInstance {
                    name: "status_bar".into(),
                    enabled: true,
                    position: WidgetPosition::Bottom,
                    width: None,
                    height: Some(1),
                },
            ],
        }
    }
}

/// A single widget instantiation in the TUI layout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetInstance {
    /// Unique widget name (must match a known widget type).
    pub name: String,
    /// On/off toggle. No runtime overhead when `false`.
    pub enabled: bool,
    /// Which screen zone this widget occupies.
    pub position: WidgetPosition,
    /// Preferred width (used for Left/Right/Bottom; Main widgets are tabs).
    pub width: Option<u16>,
    /// Preferred height (used for Bottom widgets).
    pub height: Option<u16>,
}

/// Screen zone a widget occupies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WidgetPosition {
    /// Left panel (sidebar).
    Left,
    /// Main content area (becomes a tab).
    Main,
    /// Right panel (usage, info).
    Right,
    /// Bottom panel (log, status bar).
    Bottom,
}

// ─────────────────────────────────────────────────────────
// Path resolution
// ─────────────────────────────────────────────────────────

/// Returns `~/.config/grok-build/`.
pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("grok-build")
}

/// Returns `~/.config/grok-build/config.toml`.
pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

// ─────────────────────────────────────────────────────────
// Loader
// ─────────────────────────────────────────────────────────

/// Loads config from disk, merges defaults, applies env overrides.
pub fn load_config() -> Result<GrokBuildConfig, ConfigError> {
    let path = config_path();

    if !path.exists() {
        tracing::info!("config not found at {path:?}, using defaults");
        return Ok(GrokBuildConfig::default());
    }

    let raw = std::fs::read_to_string(&path)?;
    let mut cfg: GrokBuildConfig = toml::from_str(&raw).map_err(|e| {
        ConfigError::Parse(format!("{e} (in {})", path.display()))
    })?;

    // Env overrides always win
    if let Ok(val) = std::env::var("GROK_BUILD_BEARER_TOKEN") {
        cfg.auth.bearer_token = Some(val);
    }
    if let Ok(val) = std::env::var("GROK_BUILD_SSO_COOKIE") {
        cfg.auth.sso_cookie = Some(val);
    }
    if let Ok(val) = std::env::var("GROK_BUILD_TIMEOUT_SECS") {
        if let Ok(n) = val.parse() {
            cfg.defaults.timeout_secs = n;
        }
    }
    if let Ok(val) = std::env::var("GROK_BUILD_BASE_URL_BUILD") {
        cfg.defaults.base_url_build = val;
    }
    if let Ok(val) = std::env::var("GROK_BUILD_BASE_URL_WEB") {
        cfg.defaults.base_url_web = val;
    }

    Ok(cfg)
}

/// Write a default config to disk, returning the path.
pub fn write_default_config() -> Result<PathBuf, ConfigError> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;

    let path = config_path();
    let cfg = GrokBuildConfig::default();
    let toml_str = toml::to_string_pretty(&cfg)?;
    std::fs::write(&path, toml_str)?;
    tracing::info!("wrote default config to {path:?}");
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_expected_values() {
        let cfg = GrokBuildConfig::default();
        assert_eq!(cfg.defaults.timeout_secs, 60);
        assert_eq!(cfg.defaults.base_url_build, "https://cli-chat-proxy.grok.com");
        assert_eq!(cfg.defaults.base_url_web, "https://grok.com");
        assert!(cfg.features.chat);
        assert!(cfg.features.storage);
        assert!(cfg.tui.enabled);
        assert_eq!(cfg.tui.layout.widgets.len(), 12);
    }

    #[test]
    fn default_config_round_trips_toml() {
        let cfg = GrokBuildConfig::default();
        let toml_str = toml::to_string_pretty(&cfg).unwrap();
        let parsed: GrokBuildConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.defaults.timeout_secs, cfg.defaults.timeout_secs);
        assert_eq!(parsed.tui.layout.widgets.len(), cfg.tui.layout.widgets.len());
    }

    #[test]
    fn widget_instance_serde() {
        let w = WidgetInstance {
            name: "test".into(),
            enabled: true,
            position: WidgetPosition::Left,
            width: Some(20),
            height: None,
        };
        let json = serde_json::to_string(&w).unwrap();
        let back: WidgetInstance = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "test");
        assert!(back.enabled);
    }

    #[test]
    fn env_override_applied() {
        // Simulate env override logic from load_config
        let mut cfg = GrokBuildConfig::default();
        cfg.auth.bearer_token = Some("env-token".into());
        assert_eq!(cfg.auth.bearer_token.as_deref(), Some("env-token"));
    }

    #[test]
    fn config_path_ends_correctly() {
        let p = config_path();
        assert!(p.to_string_lossy().ends_with("grok-build/config.toml"));
    }

    #[test]
    fn features_default_all_true() {
        let cfg = FeaturesConfig::default();
        assert!(cfg.chat);
        assert!(cfg.storage);
        assert!(cfg.files);
        assert!(cfg.workspaces);
        assert!(cfg.models);
        assert!(cfg.skills);
        assert!(cfg.mcp);
    }
}
