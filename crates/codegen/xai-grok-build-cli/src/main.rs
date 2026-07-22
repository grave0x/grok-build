//! `grok-build` CLI — manage config, auth, run diagnostics, and launch the TUI.

#![deny(unused)]
#![allow(clippy::print_stderr, clippy::print_stdout)]

use clap::{Parser, Subcommand};
use xai_grok_api_client::ApiError;
use xai_grok_build_config::{ConfigError, GrokBuildConfig, load_config, write_default_config};

// ─────────────────────────────────────────────────────────
// CLI
// ─────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "grok-build", about = "Grok Build CLI — manage API config & launch TUI", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scaffold default config.toml.
    Init,
    /// Auth commands.
    Auth {
        #[command(subcommand)]
        action: AuthAction,
    },
    /// Show current config (tokens masked).
    ConfigShow,
    /// Show subscription + task quota.
    Usage,
    /// API diagnostics.
    Api {
        #[command(subcommand)]
        action: ApiAction,
    },
    /// List available models.
    Models,
    /// List available skills.
    Skills,
    /// Launch the TUI.
    Tui,
    /// Send a chat message and print the response.
    Chat {
        /// Message to send.
        message: String,
        /// Model ID (default: grok-3-latest).
        #[arg(long, default_value = "grok-3-latest")]
        model: String,
    },
    /// List all known widgets and their status.
    WidgetList,
    /// Enable a widget by name.
    WidgetEnable { name: String },
    /// Disable a widget by name.
    WidgetDisable { name: String },
    /// List all feature toggles.
    FeaturesList,
    /// Set a feature toggle (true/false).
    FeaturesSet {
        name: String,
        #[arg(num_args = 1, value_parser = clap::value_parser!(bool))]
        value: bool,
    },
}

#[derive(Subcommand)]
enum AuthAction {
    /// Show auth status (token presence/validity).
    Status,
    /// Clear all stored tokens.
    Logout,
}

#[derive(Subcommand)]
enum ApiAction {
    /// Test all enabled endpoints.
    Check,
    /// Test a single endpoint group.
    Test { group: String },
}

// ─────────────────────────────────────────────────────────
// Main
// ─────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cfg = load_config().unwrap_or_else(|e| {
        eprintln!("warning: config load failed: {e}, using defaults");
        GrokBuildConfig::default()
    });

    let cli = Cli::parse();

    match cli.command {
        Command::Init => cmd_init().await?,
        Command::Auth { action } => cmd_auth(action, &cfg).await?,
        Command::ConfigShow => cmd_config_show(&cfg).await?,
        Command::Usage => cmd_usage(&cfg).await?,
        Command::Api { action } => cmd_api(action, &cfg).await?,
        Command::Models => cmd_models(&cfg).await?,
        Command::Skills => cmd_skills(&cfg).await?,
        Command::Tui => cmd_tui(&cfg).await?,
        Command::Chat { message, model } => cmd_chat(&cfg, &message, &model).await?,
        Command::WidgetList => cmd_widget_list(&cfg).await?,
        Command::WidgetEnable { name } => cmd_widget_enable(&cfg, &name, true).await?,
        Command::WidgetDisable { name } => cmd_widget_enable(&cfg, &name, false).await?,
        Command::FeaturesList => cmd_features_list(&cfg).await?,
        Command::FeaturesSet { name, value } => cmd_features_set(&cfg, &name, value).await?,
    }

    Ok(())
}

// ─────────────────────────────────────────────────────────
// Command implementations
// ─────────────────────────────────────────────────────────

async fn cmd_init() -> anyhow::Result<()> {
    let path = write_default_config()?;
    println!("✅ config written to {}", path.display());
    println!("   edit it or set GROK_BUILD_BEARER_TOKEN / GROK_BUILD_SSO_COOKIE env vars");
    Ok(())
}

async fn cmd_auth(action: AuthAction, cfg: &GrokBuildConfig) -> anyhow::Result<()> {
    match action {
        AuthAction::Status => {
            let bearer = cfg.auth.bearer_token.as_deref().unwrap_or("");
            let sso = cfg.auth.sso_cookie.as_deref().unwrap_or("");
            println!("bearer token:  {}", bool_icon(!bearer.is_empty()));
            println!("sso cookie:    {}", bool_icon(!sso.is_empty()));
            if bearer.is_empty() && sso.is_empty() {
                println!("\n  no credentials. run `grok-build init` then edit config.toml");
                println!("  or set GROK_BUILD_BEARER_TOKEN / GROK_BUILD_SSO_COOKIE");
            }
        }
        AuthAction::Logout => {
            println!("clearing tokens…");
            let mut cfg = cfg.clone();
            cfg.auth.bearer_token = None;
            cfg.auth.sso_cookie = None;
            save_config(&cfg)?;
            println!("✅ tokens cleared");
        }
    }
    Ok(())
}

async fn cmd_config_show(cfg: &GrokBuildConfig) -> anyhow::Result<()> {
    let mut display = cfg.clone();
    if let Some(ref t) = display.auth.bearer_token {
        display.auth.bearer_token = Some(mask_token(t));
    }
    if let Some(ref t) = display.auth.sso_cookie {
        display.auth.sso_cookie = Some(mask_token(t));
    }
    let json = serde_json::to_string_pretty(&display)?;
    println!("{json}");
    Ok(())
}

async fn cmd_usage(cfg: &GrokBuildConfig) -> anyhow::Result<()> {
    if !cfg.features.workspaces {
        println!("ℹ️ workspaces feature disabled");
        return Ok(());
    }
    let sso = match &cfg.auth.sso_cookie {
        Some(k) => k.clone(),
        None => {
            println!("❌ no sso cookie set");
            return Ok(());
        }
    };
    let client = xai_grok_api_client::HttpClient::new_sso(sso, cfg.defaults.timeout_secs);
    match client.web_get_subscription().await {
        Ok(sub) => {
            println!("plan:   {}", sub.plan);
            println!("status: {}", sub.status);
            if let Some(r) = sub.renews_at {
                println!("renews: {r}");
            }
        }
        Err(e) => println!("❌ subscription: {e}"),
    }
    match client.web_get_task_usage().await {
        Ok(tasks) => {
            println!("tasks:  {}/{} used", tasks.tasks_used, tasks.total_tasks);
            println!("remaining: {}", tasks.tasks_remaining);
            if let Some(r) = tasks.quota_reset_at {
                println!("reset:  {r}");
            }
        }
        Err(e) => println!("❌ task usage: {e}"),
    }
    Ok(())
}

async fn cmd_api(action: ApiAction, cfg: &GrokBuildConfig) -> anyhow::Result<()> {
    match action {
        ApiAction::Check => {
            println!("checking endpoints…");
            if let Some(token) = &cfg.auth.bearer_token {
                let client = xai_grok_api_client::HttpClient::new_bearer(
                    token.clone(),
                    cfg.defaults.timeout_secs,
                );
                if cfg.features.models {
                    let r = client.web_list_models().await;
                    print_group("build:models", &r);
                }
                if cfg.features.chat {
                    let r = client.web_list_models().await;
                    print_group("build:chat (proxy)", &r);
                }
            }
            if let Some(cookie) = &cfg.auth.sso_cookie {
                let client = xai_grok_api_client::HttpClient::new_sso(
                    cookie.clone(),
                    cfg.defaults.timeout_secs,
                );
                if cfg.features.workspaces {
                    let r = client.web_list_workspaces().await;
                    print_group("web:workspaces", &r);
                }
            }
            println!("✅ check complete");
        }
        ApiAction::Test { group } => {
            let msg = match group.as_str() {
                "chat" => test_chat(cfg).await,
                "storage" => test_storage(cfg).await,
                "workspaces" => test_workspaces(cfg).await,
                "models" => test_models(cfg).await,
                "skills" => test_skills(cfg).await,
                "files" => test_files(cfg).await,
                "mcp" => test_mcp(cfg).await,
                _ => format!("unknown group: {group}"),
            };
            println!("{msg}");
        }
    }
    Ok(())
}

async fn cmd_models(cfg: &GrokBuildConfig) -> anyhow::Result<()> {
    let token = require_bearer(cfg)?;
    let client = xai_grok_api_client::HttpClient::new_bearer(token, cfg.defaults.timeout_secs);
    match client.web_list_models().await {
        Ok(list) => {
            for m in &list {
                println!("  {} — {}", m.id, m.name);
            }
        }
        Err(e) => println!("❌ {e}"),
    }
    Ok(())
}

async fn cmd_skills(cfg: &GrokBuildConfig) -> anyhow::Result<()> {
    let cookie = require_sso(cfg)?;
    let client = xai_grok_api_client::HttpClient::new_sso(cookie, cfg.defaults.timeout_secs);
    match client.web_list_skills().await {
        Ok(list) => {
            for s in &list {
                println!("  {} — {}", s.id, s.name);
                if let Some(d) = &s.description {
                    println!("      {d}");
                }
            }
        }
        Err(e) => println!("❌ {e}"),
    }
    Ok(())
}

async fn cmd_chat(cfg: &GrokBuildConfig, message: &str, model: &str) -> anyhow::Result<()> {
    let token = require_bearer(cfg)?;
    let client = xai_grok_api_client::HttpClient::new_bearer(token, cfg.defaults.timeout_secs);
    let req = xai_grok_api_client::ChatCompletionRequest {
        model: model.into(),
        messages: vec![xai_grok_api_client::ChatMessage {
            role: "user".into(),
            content: message.into(),
        }],
        max_tokens: Some(4096),
        temperature: Some(0.7),
        stream: None,
    };
    match client.build_chat_completion(req).await {
        Ok(resp) => {
            if let Some(choice) = resp.choices.first() {
                println!("{}", choice.message.content);
            } else {
                println!("❌ empty response");
            }
            if let Some(u) = resp.usage {
                eprintln!(
                    "  tokens: {} prompt + {} completion = {} total",
                    u.prompt_tokens, u.completion_tokens, u.total_tokens
                );
            }
        }
        Err(e) => println!("❌ {e}"),
    }
    Ok(())
}

async fn cmd_tui(_cfg: &GrokBuildConfig) -> anyhow::Result<()> {
    println!("🚧 TUI not yet built — run `cargo run -p xai-grok-build-tui` once implemented");
    Ok(())
}

async fn cmd_widget_list(cfg: &GrokBuildConfig) -> anyhow::Result<()> {
    for w in &cfg.tui.layout.widgets {
        let icon = if w.enabled { "✅" } else { "⏹" };
        println!("  {icon} {:12} {:8} {:?}", w.name, format!("{:?}", w.position), w.width);
    }
    Ok(())
}

async fn cmd_widget_enable(cfg: &GrokBuildConfig, name: &str, enabled: bool) -> anyhow::Result<()> {
    let mut cfg = cfg.clone();
    if let Some(w) = cfg.tui.layout.widgets.iter_mut().find(|w| w.name == name) {
        w.enabled = enabled;
        save_config(&cfg)?;
        let action = if enabled { "enabled" } else { "disabled" };
        println!("✅ widget {name} {action}");
    } else {
        println!("❌ unknown widget: {name}");
    }
    Ok(())
}

async fn cmd_features_list(cfg: &GrokBuildConfig) -> anyhow::Result<()> {
    let f = &cfg.features;
    println!("  chat:        {}", bool_icon(f.chat));
    println!("  storage:     {}", bool_icon(f.storage));
    println!("  files:       {}", bool_icon(f.files));
    println!("  workspaces:  {}", bool_icon(f.workspaces));
    println!("  models:      {}", bool_icon(f.models));
    println!("  skills:      {}", bool_icon(f.skills));
    println!("  mcp:         {}", bool_icon(f.mcp));
    Ok(())
}

async fn cmd_features_set(cfg: &GrokBuildConfig, name: &str, value: bool) -> anyhow::Result<()> {
    let mut cfg = cfg.clone();
    let f = &mut cfg.features;
    match name {
        "chat" => f.chat = value,
        "storage" => f.storage = value,
        "files" => f.files = value,
        "workspaces" => f.workspaces = value,
        "models" => f.models = value,
        "skills" => f.skills = value,
        "mcp" => f.mcp = value,
        _ => {
            println!("❌ unknown feature: {name}");
            return Ok(());
        }
    }
    save_config(&cfg)?;
    let action = if value { "enabled" } else { "disabled" };
    println!("✅ feature {name} {action}");
    Ok(())
}

// ─────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────

fn bool_icon(v: bool) -> &'static str {
    if v { "✅" } else { "⏹" }
}

fn mask_token(s: &str) -> String {
    if s.len() <= 8 { return "***".into(); }
    format!("{}…{}", &s[..4], &s[s.len()-4..])
}

fn save_config(cfg: &GrokBuildConfig) -> Result<(), ConfigError> {
    let path = xai_grok_build_config::config_path();
    let dir = path.parent().unwrap();
    std::fs::create_dir_all(dir)?;
    let raw = toml::to_string_pretty(cfg)?;
    std::fs::write(&path, raw)?;
    Ok(())
}

fn require_bearer(cfg: &GrokBuildConfig) -> anyhow::Result<String> {
    cfg.auth.bearer_token.clone()
        .ok_or_else(|| anyhow::anyhow!("❌ no bearer token"))
}

fn require_sso(cfg: &GrokBuildConfig) -> anyhow::Result<String> {
    cfg.auth.sso_cookie.clone()
        .ok_or_else(|| anyhow::anyhow!("❌ no sso cookie"))
}

fn print_group<T: std::fmt::Debug>(label: &str, result: &Result<T, ApiError>) {
    match result {
        Ok(_) => println!("  ✅ {label}"),
        Err(e) => println!("  ❌ {label}: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_parses_init() {
        let cli = Cli::try_parse_from(["grok-build", "init"]).unwrap();
        assert!(matches!(cli.command, Command::Init));
    }

    #[test]
    fn cli_parses_auth_status() {
        let cli = Cli::try_parse_from(["grok-build", "auth", "status"]).unwrap();
        assert!(matches!(cli.command, Command::Auth { action: AuthAction::Status }));
    }

    #[test]
    fn cli_parses_auth_logout() {
        let cli = Cli::try_parse_from(["grok-build", "auth", "logout"]).unwrap();
        assert!(matches!(cli.command, Command::Auth { action: AuthAction::Logout }));
    }

    #[test]
    fn cli_parses_config_show() {
        let cli = Cli::try_parse_from(["grok-build", "config-show"]).unwrap();
        assert!(matches!(cli.command, Command::ConfigShow));
    }

    #[test]
    fn cli_parses_usage() {
        let cli = Cli::try_parse_from(["grok-build", "usage"]).unwrap();
        assert!(matches!(cli.command, Command::Usage));
    }

    #[test]
    fn cli_parses_models() {
        let cli = Cli::try_parse_from(["grok-build", "models"]).unwrap();
        assert!(matches!(cli.command, Command::Models));
    }

    #[test]
    fn cli_parses_skills() {
        let cli = Cli::try_parse_from(["grok-build", "skills"]).unwrap();
        assert!(matches!(cli.command, Command::Skills));
    }

    #[test]
    fn cli_parses_tui() {
        let cli = Cli::try_parse_from(["grok-build", "tui"]).unwrap();
        assert!(matches!(cli.command, Command::Tui));
    }

    #[test]
    fn cli_parses_chat() {
        let cli = Cli::try_parse_from(["grok-build", "chat", "hello world"]).unwrap();
        assert!(matches!(cli.command, Command::Chat { .. }));
    }

    #[test]
    fn cli_parses_chat_with_model() {
        let cli = Cli::try_parse_from(["grok-build", "chat", "hi", "--model", "grok-3-mini"]).unwrap();
        if let Command::Chat { message, model } = cli.command {
            assert_eq!(message, "hi");
            assert_eq!(model, "grok-3-mini");
        } else {
            panic!("expected Chat");
        }
    }

    #[test]
    fn cli_parses_widget_list() {
        let cli = Cli::try_parse_from(["grok-build", "widget-list"]).unwrap();
        assert!(matches!(cli.command, Command::WidgetList));
    }

    #[test]
    fn cli_parses_widget_enable() {
        let cli = Cli::try_parse_from(["grok-build", "widget-enable", "sidebar"]).unwrap();
        assert!(matches!(cli.command, Command::WidgetEnable { .. }));
    }

    #[test]
    fn cli_parses_features_list() {
        let cli = Cli::try_parse_from(["grok-build", "features-list"]).unwrap();
        assert!(matches!(cli.command, Command::FeaturesList));
    }

    #[test]
    fn cli_parses_features_set() {
        let cli = Cli::try_parse_from(["grok-build", "features-set", "chat", "false"]).unwrap();
        match cli.command {
            Command::FeaturesSet { name, value } => {
                assert_eq!(name, "chat");
                assert!(!value);
            }
            _ => panic!("expected FeaturesSet"),
        }
    }

    #[test]
    fn cli_parses_api_check() {
        let cli = Cli::try_parse_from(["grok-build", "api", "check"]).unwrap();
        assert!(matches!(cli.command, Command::Api { action: ApiAction::Check }));
    }

    #[test]
    fn cli_parses_api_test() {
        let cli = Cli::try_parse_from(["grok-build", "api", "test", "chat"]).unwrap();
        match cli.command {
            Command::Api { action: ApiAction::Test { group } } => assert_eq!(group, "chat"),
            _ => panic!("expected Api::Test"),
        }
    }

    #[test]
    fn cli_help_succeeds() {
        let result = Cli::try_parse_from(["grok-build", "--help"]);
        assert!(result.is_err()); // clap prints help and exits with error in this mode
    }

    #[test]
    fn mask_token_short() {
        assert_eq!(mask_token("ab"), "***");
    }

    #[test]
    fn mask_token_long() {
        let masked = mask_token("abcdefghijklmnop");
        assert!(masked.starts_with("abcd"));
        assert!(masked.ends_with("mnop"));
        assert!(masked.contains("…"));
    }
}

// ─────────────────────────────────────────────────────────
// API test helpers
// ─────────────────────────────────────────────────────────

async fn test_chat(cfg: &GrokBuildConfig) -> String {
    let Ok(token) = require_bearer(cfg) else { return "❌ no bearer token".into() };
    let client = xai_grok_api_client::HttpClient::new_bearer(token, cfg.defaults.timeout_secs);
    match client.web_list_models().await {
        Ok(_) => "✅ chat API reachable".into(),
        Err(e) => format!("❌ {e}"),
    }
}

async fn test_storage(cfg: &GrokBuildConfig) -> String {
    let Ok(token) = require_bearer(cfg) else { return "❌ no bearer token".into() };
    let client = xai_grok_api_client::HttpClient::new_bearer(token, cfg.defaults.timeout_secs);
    match client.storage_check_exists(&["test-check".into()]).await {
        Ok(r) => format!("✅ storage reachable ({} existing, {} missing)", r.existing.len(), r.missing.len()),
        Err(e) => format!("❌ {e}"),
    }
}

async fn test_workspaces(cfg: &GrokBuildConfig) -> String {
    let Ok(cookie) = require_sso(cfg) else { return "❌ no sso cookie".into() };
    let client = xai_grok_api_client::HttpClient::new_sso(cookie, cfg.defaults.timeout_secs);
    match client.web_list_workspaces().await {
        Ok(_) => "✅ workspaces API reachable".into(),
        Err(e) => format!("❌ {e}"),
    }
}

async fn test_models(cfg: &GrokBuildConfig) -> String {
    let Ok(token) = require_bearer(cfg) else { return "❌ no bearer token".into() };
    let client = xai_grok_api_client::HttpClient::new_bearer(token, cfg.defaults.timeout_secs);
    match client.web_list_models().await {
        Ok(_) => "✅ models API reachable".into(),
        Err(e) => format!("❌ {e}"),
    }
}

async fn test_skills(cfg: &GrokBuildConfig) -> String {
    let Ok(cookie) = require_sso(cfg) else { return "❌ no sso cookie".into() };
    let client = xai_grok_api_client::HttpClient::new_sso(cookie, cfg.defaults.timeout_secs);
    match client.web_list_skills().await {
        Ok(_) => "✅ skills API reachable".into(),
        Err(e) => format!("❌ {e}"),
    }
}

async fn test_files(cfg: &GrokBuildConfig) -> String {
    let Ok(cookie) = require_sso(cfg) else { return "❌ no sso cookie".into() };
    let client = xai_grok_api_client::HttpClient::new_sso(cookie, cfg.defaults.timeout_secs);
    match client.web_file_content("").await {
        Err(ApiError::Http { status, .. }) if status == 400 => {
            "✅ files API reachable (expected 400)".into()
        }
        Err(e) => format!("❌ {e}"),
        Ok(_) => "✅ files API reachable".into(),
    }
}

async fn test_mcp(cfg: &GrokBuildConfig) -> String {
    let Ok(cookie) = require_sso(cfg) else { return "❌ no sso cookie".into() };
    let client = xai_grok_api_client::HttpClient::new_sso(cookie, cfg.defaults.timeout_secs);
    match client.web_mcp_list_resources().await {
        Ok(r) => format!("✅ MCP resources: {} items", r.len()),
        Err(e) => format!("❌ {e}"),
    }
}
