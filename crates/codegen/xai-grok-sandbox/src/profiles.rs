//! Sandbox profiles. Built-in: `workspace`, `devbox`, `read-only`, `strict`,
//! `off`. Custom profiles via `~/.grok/sandbox.toml` or `.grok/sandbox.toml`.
//! A custom profile's `deny` list is kernel-enforced (read + write/rename) on
//! both platforms.

#[cfg(all(feature = "enforce", unix))]
use nono::{AccessMode, CapabilitySet};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[cfg(all(feature = "enforce", unix))]
use crate::deny::{
    apply_deny_globs_to_capability_set, apply_deny_paths_to_capability_set, effective_deny_paths,
    partition_deny_entries,
};
use crate::paths::grok_home;
#[cfg(all(feature = "enforce", unix))]
use crate::paths::{
    DEVICE_DIRS, DEVICE_FILES, essential_writable_paths, essential_writable_paths_minimal,
};

/// A resolved sandbox profile ready to be converted to a `CapabilitySet`.
#[derive(Debug, Clone)]
pub struct SandboxProfile {
    /// Display name
    pub name: String,
    /// Paths the agent can read (but not write)
    pub read_only: Vec<PathBuf>,
    /// Paths the agent can read and write
    pub read_write: Vec<PathBuf>,
    /// Paths denied entirely (overrides read_only/read_write)
    pub deny: Vec<PathBuf>,
    /// Whether to grant read access to the entire filesystem by default
    pub default_read: bool,
    /// Whether child processes should have network blocked
    pub restrict_network: bool,
    /// Size of tmpfs mounted at /tmp (e.g. "512M", "2G")
    pub tmpfs_size: Option<String>,
    /// CPU quota for child processes (e.g. "50%" or cgroup cpu.max)
    pub cpu_quota: Option<String>,
    /// Memory limit for child processes (e.g. "1G", "512M")
    pub memory_max: Option<String>,
    /// IO weight for child processes (e.g. "100")
    pub io_weight: Option<String>,
    /// Network allowlist — IPs/domains allowed despite restrict_network
    pub network_allow: Vec<String>,
    /// Network denylist — IPs/domains denied
    pub network_deny: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ProfileConfig {
    #[serde(default)]
    pub extends: Option<String>,
    #[serde(default)]
    pub restrict_network: Option<bool>,
    #[serde(default)]
    pub read_only: Vec<String>,
    #[serde(default)]
    pub read_write: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
    #[serde(default)]
    pub network_allow: Vec<String>,
    #[serde(default)]
    pub network_deny: Vec<String>,
    #[serde(default)]
    pub cpu_quota: Option<String>,
    #[serde(default)]
    pub memory_max: Option<String>,
    #[serde(default)]
    pub io_weight: Option<String>,
    #[serde(default)]
    pub tmpfs_size: Option<String>,
    #[serde(default)]
    pub proxy: Option<String>,
}

/// Profile selection rules: explicit name, command-prefix matching, and
/// repo-based matching. Loaded from `[sandbox]` in config.toml.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SandboxSelectionConfig {
    /// Explicitly chosen profile name (highest priority)
    #[serde(default)]
    pub profile: Option<String>,
    /// Map of command prefix → profile name (e.g. "npm publish" → "strict")
    #[serde(default)]
    pub profile_by_command: HashMap<String, String>,
    /// Map of repo URL → profile name
    #[serde(default)]
    pub profile_by_repo: HashMap<String, String>,
    /// Fallback profile name when nothing else matches
    #[serde(default)]
    pub default_profile: Option<String>,
}

impl Default for SandboxSelectionConfig {
    fn default() -> Self {
        Self {
            profile: None,
            profile_by_command: HashMap::new(),
            profile_by_repo: HashMap::new(),
            default_profile: Some("workspace".to_string()),
        }
    }
}

impl SandboxSelectionConfig {
    /// Resolve a profile name from the selection rules.
    ///
    /// Priority:
    /// 1. Explicit `profile` field
    /// 2. `profile_by_command` — longest prefix match against `command`
    /// 3. `profile_by_repo` — exact match or suffix match against `repo_url`
    /// 4. `default_profile`
    pub fn resolve_profile_name(
        &self,
        command: Option<&str>,
        repo_url: Option<&str>,
    ) -> ProfileName {
        // 1. Explicit profile
        if let Some(name) = &self.profile {
            let parsed: ProfileName = name.parse().unwrap_or(ProfileName::Custom(name.clone()));
            return parsed;
        }

        // 2. Command prefix match
        if let Some(cmd) = command {
            // Find longest matching command prefix
            let mut best_len = 0usize;
            let mut best_name: Option<&str> = None;
            for (prefix, pname) in &self.profile_by_command {
                if cmd.starts_with(prefix) && prefix.len() > best_len {
                    best_len = prefix.len();
                    best_name = Some(pname);
                }
            }
            if let Some(name) = best_name {
                return name.parse().unwrap_or(ProfileName::Custom(name.to_string()));
            }
        }

        // 3. Repo URL match
        if let Some(url) = repo_url {
            // Exact match first, then suffix match
            if let Some(name) = self.profile_by_repo.get(url) {
                return name.parse().unwrap_or(ProfileName::Custom(name.to_string()));
            }
            // Suffix match: find longest matching repo URL suffix
            let mut best_len = 0usize;
            let mut best_name: Option<&str> = None;
            for (repo_key, pname) in &self.profile_by_repo {
                if url.ends_with(repo_key) && repo_key.len() > best_len {
                    best_len = repo_key.len();
                    best_name = Some(pname);
                }
            }
            if let Some(name) = best_name {
                return name.parse().unwrap_or(ProfileName::Custom(name.to_string()));
            }
        }

        // 4. Default
        self.default_profile
            .as_deref()
            .unwrap_or("workspace")
            .parse()
            .unwrap_or(ProfileName::Workspace)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct SandboxConfig {
    #[serde(default)]
    pub profiles: HashMap<String, ProfileConfig>,
    #[serde(default)]
    pub selection: SandboxSelectionConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ProfileName {
    #[default]
    Workspace,
    Devbox,
    ReadOnly,
    Strict,
    Off,
    Custom(String),
}

/// All names that are currently recognized as built-in profile variants.
/// Used at config-merge time to warn when a project profile shadows a name
/// that the parser will treat as a built-in rather than a custom variant.
///
/// When adding a new variant to [`ProfileName`], add its string representation
/// here so the merge guard stays in sync.
pub fn built_in_profile_names() -> &'static [&'static str] {
    &["workspace", "devbox", "read-only", "readonly", "strict", "off", "none"]
}

impl ProfileName {
    pub fn restricts_network(&self) -> bool {
        matches!(self, Self::ReadOnly | Self::Strict)
    }

    /// Resolve network restriction from config (handles Custom profiles).
    pub fn restricts_network_resolved(&self, config: &SandboxConfig) -> bool {
        match self {
            Self::ReadOnly | Self::Strict => true,
            Self::Workspace | Self::Devbox | Self::Off => false,
            Self::Custom(name) => config
                .profiles
                .get(name)
                .and_then(|p| p.restrict_network)
                .unwrap_or(false),
        }
    }
}

impl std::fmt::Display for ProfileName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Workspace => write!(f, "workspace"),
            Self::Devbox => write!(f, "devbox"),
            Self::ReadOnly => write!(f, "read-only"),
            Self::Strict => write!(f, "strict"),
            Self::Off => write!(f, "off"),
            Self::Custom(name) => write!(f, "{name}"),
        }
    }
}

impl std::str::FromStr for ProfileName {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "workspace" => Ok(Self::Workspace),
            "devbox" => Ok(Self::Devbox),
            "read-only" | "readonly" => Ok(Self::ReadOnly),
            "strict" => Ok(Self::Strict),
            "off" | "none" => Ok(Self::Off),
            // Anything else is treated as a custom profile name.
            // Validation happens when we try to load it from config.
            other => Ok(Self::Custom(other.to_string())),
        }
    }
}

/// Load sandbox config from `~/.grok/sandbox.toml` and `.grok/sandbox.toml`.
///
/// Project config may **add** new profile names only. It cannot redefine a
/// name already present in the global config — last-write-wins would let a
/// malicious workspace hollow out a user/enterprise custom profile (e.g.
/// empty `deny` / broad `read_write`) while keeping the trusted name.
pub fn load_sandbox_config(workspace: &Path) -> SandboxConfig {
    let mut config = SandboxConfig::default();

    // Global config: ~/.grok/sandbox.toml
    let global_path = grok_home().join("sandbox.toml");
    if let Some(global) = load_config_file(&global_path) {
        config = global;
    }

    // Project config: <workspace>/.grok/sandbox.toml (additive only)
    let project_path = workspace.join(".grok").join("sandbox.toml");
    if let Some(project) = load_config_file(&project_path) {
        merge_project_profiles(&mut config, project);
    }

    config
}

pub fn sandbox_profile_conflicts(workspace: &Path) -> Vec<String> {
    let global = load_config_file(&grok_home().join("sandbox.toml")).unwrap_or_default();
    let project =
        load_config_file(&workspace.join(".grok").join("sandbox.toml")).unwrap_or_default();
    mismatched_profile_names(&global, &project)
}

fn mismatched_profile_names(global: &SandboxConfig, project: &SandboxConfig) -> Vec<String> {
    let mut names: Vec<String> = project
        .profiles
        .iter()
        .filter(|(name, _)| matches!(name.parse(), Ok(ProfileName::Custom(_))))
        .filter_map(|(name, project_profile)| {
            global
                .profiles
                .get(name)
                .filter(|global_profile| *global_profile != project_profile)
                .map(|_| name.to_owned())
        })
        .collect();
    names.sort_unstable();
    names
}

/// Merge project profiles into `config`. Names already defined globally are
/// ignored so a workspace cannot replace a global custom profile's policy.
fn merge_project_profiles(config: &mut SandboxConfig, project: SandboxConfig) {
    for (name, profile) in project.profiles {
        if built_in_profile_names().contains(&name.as_str()) {
            tracing::warn!(
                name = %name,
                "Project sandbox profile '{name}' shadows a reserved built-in name; \
                 it will be ignored in a future version that defines it as a built-in. \
                 Rename the profile to avoid confusion."
            );
        }
        config.profiles.entry(name).or_insert(profile);
    }
}

fn load_config_file(path: &Path) -> Option<SandboxConfig> {
    let content = std::fs::read_to_string(path).ok()?;
    match toml::from_str(&content) {
        Ok(config) => Some(config),
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "Failed to parse sandbox config");
            None
        }
    }
}

#[cfg(all(feature = "enforce", unix))]
impl ProfileName {
    /// Convert this profile into a nono `CapabilitySet` for the given workspace.
    pub fn to_capability_set(&self, workspace: &Path) -> anyhow::Result<CapabilitySet> {
        let config = load_sandbox_config(workspace);
        self.to_capability_set_with_config(workspace, &config)
    }

    /// Convert using an already-loaded config (avoids re-reading disk).
    ///
    /// A custom profile's own `deny` list is kernel-enforced (read + write/rename)
    /// on top of the base profile.
    pub fn to_capability_set_with_config(
        &self,
        workspace: &Path,
        config: &SandboxConfig,
    ) -> anyhow::Result<CapabilitySet> {
        if *self == Self::Off {
            return Ok(CapabilitySet::new());
        }

        // Resolve to a SandboxProfile
        let profile = self.resolve(workspace, config)?;

        // Build CapabilitySet from the resolved profile
        let mut caps = CapabilitySet::new();

        // Default read access
        if profile.default_read {
            caps = caps.allow_path("/", AccessMode::Read)?;
        }

        // Explicit read-only paths — skip non-existent (nothing to read)
        for path in &profile.read_only {
            if !path.exists() {
                continue;
            }
            let Some(path_str) = path.to_str() else {
                tracing::warn!(path = ?path, "Skipping non-UTF8 read_only path");
                continue;
            };
            caps = caps.allow_path(path_str, AccessMode::Read)?;
        }

        // Read-write paths. nono/Landlock need the directory to exist at
        // apply time (it opens an O_PATH fd), but new files within it can
        // be created freely after the sandbox is applied. Pre-create
        // directories like ~/.grok/ that may not exist on first run.
        for path in &profile.read_write {
            if !path.exists() && std::fs::create_dir_all(path).is_err() {
                tracing::warn!(path = ?path, "read_write path does not exist and could not be created, skipping");
                continue;
            }
            let Some(path_str) = path.to_str() else {
                tracing::warn!(path = ?path, "Skipping non-UTF8 read_write path");
                continue;
            };
            caps = caps.allow_path(path_str, AccessMode::ReadWrite)?;
        }

        // Device special files (character devices like /dev/null, /dev/tty, etc.).
        for dev in DEVICE_FILES {
            let p = Path::new(dev);
            if !p.exists() {
                continue;
            }
            if let Err(e) = caps.allow_file_mut(p, AccessMode::ReadWrite) {
                tracing::warn!(path = dev, error = %e, "Could not allow device file");
            }
        }
        // Device directories (e.g. /dev/pts for PTY slaves on Linux).
        for dev in DEVICE_DIRS {
            let p = Path::new(dev);
            if p.exists() && p.is_dir() {
                caps = caps.allow_path(dev, AccessMode::ReadWrite)?;
            }
        }

        // Kernel deny (read+write): macOS Seatbelt rules; Linux via bwrap bind-over.
        // The effective deny set is the profile's own `deny` (custom profiles only;
        // built-ins carry an empty `deny`). An empty set means there is nothing to
        // enforce. Keying on emptiness rather than profile type avoids enforcing
        // unintentional denies.
        //
        // Split exact paths from globs: exact paths keep the literal/subpath flow;
        // globs become anchored Seatbelt regexes on macOS (a no-op here on Linux,
        // where they are expanded and bound over at bwrap re-exec).
        let (exact_deny, glob_deny) = partition_deny_entries(&profile.deny);
        let all_denied = effective_deny_paths(workspace, &exact_deny);
        if !all_denied.is_empty() {
            apply_deny_paths_to_capability_set(&mut caps, &all_denied)?;
        }
        if !glob_deny.is_empty() {
            apply_deny_globs_to_capability_set(&mut caps, workspace, &glob_deny)?;
        }

        Ok(caps)
    }

    /// Resolve this profile into a fully-specified `SandboxProfile` for logging.
    pub fn resolve_profile(
        &self,
        workspace: &Path,
        config: &SandboxConfig,
    ) -> anyhow::Result<SandboxProfile> {
        self.resolve(workspace, config)
    }

    fn resolve(&self, workspace: &Path, config: &SandboxConfig) -> anyhow::Result<SandboxProfile> {
        match self {
            // Selected `off` is handled before resolve (empty CapabilitySet /
            // early return in apply). Reaching here is almost always a custom
            // profile with `extends = "off"` / `"none"` — return Err, never panic.
            Self::Off => anyhow::bail!(
                "sandbox profile 'off' cannot be resolved as a base profile; \
                 choose a built-in base (workspace, devbox, read-only, strict)"
            ),

            Self::Workspace => Ok(SandboxProfile {
                name: "workspace".to_string(),
                read_only: vec![],
                read_write: essential_writable_paths(workspace),
                deny: vec![],
                default_read: true,
                restrict_network: false,
                tmpfs_size: None,
                cpu_quota: None,
                memory_max: None,
                io_weight: None,
                network_allow: vec![],
                network_deny: vec![],
            }),

            Self::Devbox => {
                // Everything writable except /data. Enumerate top-level
                // dirs and skip the exclusion list. Can't grant "/" because
                // Landlock has no deny_path — sub-path exceptions are
                // only possible by not granting the parent.
                //
                // /data is excluded from read_write here (so it is not writable)
                // but is deliberately NOT a kernel-deny: it stays readable via
                // default_read, and its Linux write-deny comes from the
                // bwrap_reexec_command(&["/data"]) re-exec, not from profile.deny.
                // Keeping deny empty stops a custom profile that extends devbox
                // from inheriting /data into the enforced kernel-deny set.
                let exclude = [PathBuf::from("/data")];
                let mut read_write = vec![workspace.to_path_buf()];
                if let Ok(entries) = std::fs::read_dir("/") {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if exclude.contains(&path) {
                            continue;
                        }
                        // Skip virtual filesystems (handled separately)
                        if matches!(path.to_str(), Some("/proc" | "/sys" | "/dev")) {
                            continue;
                        }
                        if path.is_dir() {
                            read_write.push(path);
                        }
                    }
                }
                Ok(SandboxProfile {
                    name: "devbox".to_string(),
                    read_only: vec![],
                    read_write,
                    deny: vec![],
                    default_read: true,
                    restrict_network: false,
                    tmpfs_size: None,
                    cpu_quota: None,
                    memory_max: None,
                    io_weight: None,
                    network_allow: vec![],
                    network_deny: vec![],
                })
            }

            Self::ReadOnly => Ok(SandboxProfile {
                name: "read-only".to_string(),
                read_only: vec![],
                read_write: essential_writable_paths_minimal(),
                deny: vec![],
                default_read: true,
                restrict_network: true,
                tmpfs_size: None,
                cpu_quota: None,
                memory_max: None,
                io_weight: None,
                network_allow: vec![],
                network_deny: vec![],
            }),

            Self::Strict => {
                let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/root"));
                let system_read: Vec<PathBuf> = [
                    "/usr", "/lib", "/lib64", "/bin", "/sbin", "/etc", "/dev", "/proc", "/sys",
                    "/tmp",
                    // Landlock realpath: /etc/resolv.conf often → /run/systemd/resolve/…
                    "/run",
                    // NSS/SSSD (and similar) under /var — needed beyond resolv.conf alone
                    "/var",
                    // macOS-specific paths (filtered by exists() below)
                    "/System",  // Security framework, dylibs, TLS certificates
                    "/Library", // System-wide frameworks
                    "/private", // Real path behind /etc, /tmp, /var symlinks
                ]
                .iter()
                .map(PathBuf::from)
                .filter(|p| p.exists())
                // ~/Library is needed for macOS keychain access (TLS cert validation)
                .chain(std::iter::once(home.join("Library")))
                .filter(|p| p.exists())
                .chain(std::iter::once(workspace.to_path_buf()))
                .collect();

                Ok(SandboxProfile {
                    name: "strict".to_string(),
                    read_only: system_read,
                    read_write: essential_writable_paths(workspace),
                    deny: vec![],
                    default_read: false,
                    restrict_network: true,
                    tmpfs_size: None,
                    cpu_quota: None,
                    memory_max: None,
                    io_weight: None,
                    network_allow: vec![],
                    network_deny: vec![],
                })
            }

            Self::Custom(name) => {
                let profile_config = config.profiles.get(name).ok_or_else(|| {
                    anyhow::anyhow!(
                        "Custom sandbox profile '{name}' not found. \
                         Define it in ~/.grok/sandbox.toml or .grok/sandbox.toml:\n\n\
                         [profiles.{name}]\n\
                         extends = \"workspace\"\n\
                         read_only = [\"/data\"]\n"
                    )
                })?;

                // Start from the base profile if `extends` is set
                let mut profile = if let Some(base_name) = &profile_config.extends {
                    let base: ProfileName = base_name.parse().map_err(|e: String| {
                        anyhow::anyhow!("Profile '{name}' extends invalid base: {e}")
                    })?;
                    if matches!(base, Self::Off) {
                        anyhow::bail!(
                            "Profile '{name}' extends '{base_name}', but 'off'/'none' \
                             is not a valid base profile"
                        );
                    }
                    if matches!(base, Self::Custom(_)) {
                        anyhow::bail!(
                            "Profile '{name}' extends '{base_name}', but custom profiles \
                             cannot extend other custom profiles (only built-ins)"
                        );
                    }
                    base.resolve(workspace, config)?
                } else {
                    // Default: start from workspace
                    Self::Workspace.resolve(workspace, config)?
                };

                profile.name = name.clone();

                // Apply overrides from the custom config
                if let Some(restrict_net) = profile_config.restrict_network {
                    profile.restrict_network = restrict_net;
                }

                // Add custom read-only paths
                for path_str in &profile_config.read_only {
                    profile.read_only.push(PathBuf::from(path_str));
                }

                // Add custom read-write paths
                for path_str in &profile_config.read_write {
                    profile.read_write.push(PathBuf::from(path_str));
                }

                // Add custom deny paths
                for path_str in &profile_config.deny {
                    profile.deny.push(PathBuf::from(path_str));
                }

                // Override resource limit fields
                if let Some(v) = &profile_config.tmpfs_size {
                    profile.tmpfs_size = Some(v.clone());
                }
                if let Some(v) = &profile_config.cpu_quota {
                    profile.cpu_quota = Some(v.clone());
                }
                if let Some(v) = &profile_config.memory_max {
                    profile.memory_max = Some(v.clone());
                }
                if let Some(v) = &profile_config.io_weight {
                    profile.io_weight = Some(v.clone());
                }

                // Override network filter fields
                if !profile_config.network_allow.is_empty() {
                    profile.network_allow = profile_config.network_allow.clone();
                }
                if !profile_config.network_deny.is_empty() {
                    profile.network_deny = profile_config.network_deny.clone();
                }

                Ok(profile)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_profile_names() {
        assert_eq!(
            "workspace".parse::<ProfileName>().unwrap(),
            ProfileName::Workspace
        );
        assert_eq!(
            "devbox".parse::<ProfileName>().unwrap(),
            ProfileName::Devbox
        );
        assert_eq!(
            "read-only".parse::<ProfileName>().unwrap(),
            ProfileName::ReadOnly
        );
        assert_eq!(
            "readonly".parse::<ProfileName>().unwrap(),
            ProfileName::ReadOnly
        );
        assert_eq!(
            "strict".parse::<ProfileName>().unwrap(),
            ProfileName::Strict
        );
        assert_eq!("off".parse::<ProfileName>().unwrap(), ProfileName::Off);
        assert_eq!("none".parse::<ProfileName>().unwrap(), ProfileName::Off);
        // Unknown names become Custom profiles
        assert_eq!(
            "my-custom-profile".parse::<ProfileName>().unwrap(),
            ProfileName::Custom("my-custom-profile".to_string())
        );
    }

    #[test]
    fn display_roundtrip() {
        for profile in [
            ProfileName::Workspace,
            ProfileName::Devbox,
            ProfileName::ReadOnly,
            ProfileName::Strict,
            ProfileName::Off,
        ] {
            let s = profile.to_string();
            let parsed: ProfileName = s.parse().unwrap();
            assert_eq!(parsed, profile);
        }
    }

    #[test]
    fn display_custom() {
        let p = ProfileName::Custom("my-custom".to_string());
        assert_eq!(p.to_string(), "my-custom");
    }

    #[test]
    fn network_restriction() {
        assert!(!ProfileName::Workspace.restricts_network());
        assert!(!ProfileName::Devbox.restricts_network());
        assert!(ProfileName::ReadOnly.restricts_network());
        assert!(ProfileName::Strict.restricts_network());
        assert!(!ProfileName::Off.restricts_network());
    }

    #[test]
    #[cfg(all(feature = "enforce", unix))]
    fn strict_allowlist_includes_run_and_var_when_present() {
        // Regression: /run (resolv realpath) + /var (NSS/SSSD) when present.
        let workspace = std::env::temp_dir();
        let profile = ProfileName::Strict
            .resolve_profile(&workspace, &SandboxConfig::default())
            .expect("strict resolves");
        assert!(!profile.default_read);
        if PathBuf::from("/run").exists() {
            assert!(
                profile.read_only.iter().any(|p| p == Path::new("/run")),
                "strict read_only must include exact /run for systemd-resolved DNS; got {:?}",
                profile.read_only
            );
        }
        if PathBuf::from("/var").exists() {
            assert!(
                profile.read_only.iter().any(|p| p == Path::new("/var")),
                "strict read_only must include exact /var for NSS/SSSD; got {:?}",
                profile.read_only
            );
        }
    }

    #[test]
    #[cfg(all(feature = "enforce", unix))]
    fn base_profile_capability_set_builds() {
        // A base profile with no `deny` builds a CapabilitySet without erroring.
        let workspace = std::env::current_dir().unwrap();
        let config = SandboxConfig::default();
        let result = ProfileName::Workspace.to_capability_set_with_config(&workspace, &config);
        assert!(result.is_ok(), "Failed: {:?}", result.err());
    }

    #[test]
    #[cfg(all(feature = "enforce", unix))]
    fn custom_profile_from_config() {
        let workspace = std::env::current_dir().unwrap();
        let config = SandboxConfig {
            profiles: HashMap::from([(
                "project".to_string(),
                ProfileConfig {
                    extends: Some("workspace".to_string()),
                    restrict_network: Some(true),
                    read_only: vec!["/data".to_string()],
                    read_write: vec![],
                    deny: vec!["/data/private".to_string()],
                    network_allow: vec![],
                    network_deny: vec![],
                    cpu_quota: None,
                    memory_max: None,
                    io_weight: None,
                    tmpfs_size: None,
                    proxy: None,
                },
            )]),
            selection: SandboxSelectionConfig::default(),
        };

        let profile = ProfileName::Custom("project".to_string());
        let result = profile.to_capability_set_with_config(&workspace, &config);
        assert!(result.is_ok(), "Failed: {:?}", result.err());
    }

    #[test]
    #[cfg(all(feature = "enforce", unix))]
    fn custom_extends_devbox_has_no_data_in_deny() {
        // Regression: devbox excludes /data via a local list, not profile.deny, so
        // a custom profile extending devbox must not inherit /data into the kernel
        // deny set (which would wrongly read-deny /data and force fail-closed).
        let workspace = std::env::current_dir().unwrap();
        let config = SandboxConfig {
            profiles: HashMap::from([(
                "mydev".to_string(),
                ProfileConfig {
                    extends: Some("devbox".to_string()),
                    restrict_network: None,
                    read_only: vec![],
                    read_write: vec![],
                    deny: vec![],
                    network_allow: vec![],
                    network_deny: vec![],
                    cpu_quota: None,
                    memory_max: None,
                    io_weight: None,
                    tmpfs_size: None,
                    proxy: None,
                },
            )]),
            selection: SandboxSelectionConfig::default(),
        };

        let profile = ProfileName::Custom("mydev".to_string());
        let resolved = profile.resolve_profile(&workspace, &config).unwrap();
        assert!(
            !resolved.deny.contains(&PathBuf::from("/data")),
            "custom profile extending devbox must not inherit /data into deny: {:?}",
            resolved.deny
        );
    }

    #[test]
    #[cfg(all(feature = "enforce", unix))]
    fn custom_profile_not_found() {
        let workspace = std::env::current_dir().unwrap();
        let config = SandboxConfig::default();

        let profile = ProfileName::Custom("nonexistent".to_string());
        let result = profile.to_capability_set_with_config(&workspace, &config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not found"), "Unexpected error: {err}");
    }

    #[test]
    fn mismatched_profile_names_reports_only_changed_custom_profiles() {
        let profile = |restrict_network| ProfileConfig {
            extends: Some("workspace".to_string()),
            restrict_network: Some(restrict_network),
            read_only: vec![],
            read_write: vec![],
            deny: vec![],
            network_allow: vec![],
            network_deny: vec![],
            cpu_quota: None,
            memory_max: None,
            io_weight: None,
            tmpfs_size: None,
            proxy: None,
        };
        let global = SandboxConfig {
            profiles: HashMap::from([
                ("dev".to_string(), profile(false)),
                ("same".to_string(), profile(false)),
            ]),
            selection: SandboxSelectionConfig::default(),
        };
        let project = SandboxConfig {
            profiles: HashMap::from([
                ("dev".to_string(), profile(true)),
                ("same".to_string(), profile(false)),
                ("project-only".to_string(), profile(true)),
                ("devbox".to_string(), profile(true)),
            ]),
            selection: SandboxSelectionConfig::default(),
        };

        assert_eq!(mismatched_profile_names(&global, &project), vec!["dev"]);
    }

    #[test]
    fn built_in_names_are_in_sync_with_parser() {
        // Every name in built_in_profile_names() must parse to a non-Custom variant.
        // This catches drift when someone adds a new built-in but forgets to update
        // the registry.
        for name in built_in_profile_names() {
            let parsed: ProfileName = name.parse().unwrap();
            assert!(
                !matches!(parsed, ProfileName::Custom(_)),
                "built_in_profile_names() includes '{name}' but from_str returns Custom — \
                 add a match arm to ProfileName::from_str"
            );
        }
    }

    #[test]
    fn merge_project_profiles_warns_on_built_in_name() {
        // A project profile named after a built-in triggers a warning at
        // merge time. The profile IS still inserted (it's additive, and
        // there's no global name to protect), but the user is told to
        // rename it before a future version defines that built-in.
        let mut config = SandboxConfig::default();
        let project = SandboxConfig {
            profiles: HashMap::from([(
                "devbox".to_string(),
                ProfileConfig {
                    extends: Some("workspace".to_string()),
                    restrict_network: None,
                    read_only: vec![],
                    read_write: vec![],
                    deny: vec![],
                    network_allow: vec![],
                    network_deny: vec![],
                    cpu_quota: None,
                    memory_max: None,
                    io_weight: None,
                    tmpfs_size: None,
                    proxy: None,
                },
            )]),
            selection: SandboxSelectionConfig::default(),
        };
        merge_project_profiles(&mut config, project);
        assert!(
            config.profiles.contains_key("devbox"),
            "project profile with built-in name must still be inserted"
        );
    }

    #[test]
    fn parse_toml_config() {
        let toml_str = r#"
[profiles.devbox]
extends = "workspace"
restrict_network = true
read_only = ["/data"]
deny = ["/data/private"]

[profiles.ci]
extends = "strict"
read_write = ["/tmp/ci-artifacts"]
"#;
        let config: SandboxConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.profiles.len(), 2);
        assert!(config.profiles.contains_key("devbox"));
        assert!(config.profiles.contains_key("ci"));
        assert_eq!(config.profiles["devbox"].read_only, vec!["/data"]);
        assert_eq!(config.profiles["devbox"].deny, vec!["/data/private"]);
    }

    #[test]
    fn project_cannot_redefine_global_profile() {
        // Global "secure" with a real deny list must win over a project hollow-out.
        let mut config = SandboxConfig {
            profiles: HashMap::from([(
                "secure".to_string(),
                ProfileConfig {
                    extends: Some("workspace".to_string()),
                    restrict_network: Some(true),
                    read_only: vec![],
                    read_write: vec![],
                    deny: vec!["/home/user/.ssh".to_string()],
                    network_allow: vec![],
                    network_deny: vec![],
                    cpu_quota: None,
                    memory_max: None,
                    io_weight: None,
                    tmpfs_size: None,
                    proxy: None,
                },
            )]),
            selection: SandboxSelectionConfig::default(),
        };
        let project = SandboxConfig {
            profiles: HashMap::from([
                (
                    "secure".to_string(),
                    ProfileConfig {
                        extends: Some("workspace".to_string()),
                        restrict_network: Some(false),
                        read_only: vec![],
                        read_write: vec!["/".to_string()],
                        deny: vec![],
                        network_allow: vec![],
                        network_deny: vec![],
                        cpu_quota: None,
                        memory_max: None,
                        io_weight: None,
                        tmpfs_size: None,
                        proxy: None,
                    },
                ),
                (
                    "project-only".to_string(),
                    ProfileConfig {
                        extends: Some("workspace".to_string()),
                        restrict_network: None,
                        read_only: vec![],
                        read_write: vec![],
                        deny: vec!["./secrets".to_string()],
                        network_allow: vec![],
                        network_deny: vec![],
                        cpu_quota: None,
                        memory_max: None,
                        io_weight: None,
                        tmpfs_size: None,
                        proxy: None,
                    },
                ),
            ]),
            selection: SandboxSelectionConfig::default(),
        };

        merge_project_profiles(&mut config, project);

        assert_eq!(
            config.profiles["secure"].deny,
            vec!["/home/user/.ssh".to_string()],
            "global deny must be preserved"
        );
        assert_eq!(config.profiles["secure"].restrict_network, Some(true));
        assert!(
            config.profiles["secure"].read_write.is_empty(),
            "project must not widen global read_write"
        );
        assert!(
            config.profiles.contains_key("project-only"),
            "new project-only profile names are still allowed"
        );
    }

    #[test]
    #[cfg(all(feature = "enforce", unix))]
    fn extends_off_returns_err_not_panic() {
        let workspace = std::env::current_dir().unwrap();
        let config = SandboxConfig {
            profiles: HashMap::from([(
                "broken".to_string(),
                ProfileConfig {
                    extends: Some("off".to_string()),
                    restrict_network: None,
                    read_only: vec![],
                    read_write: vec![],
                    deny: vec![],
                    network_allow: vec![],
                    network_deny: vec![],
                    cpu_quota: None,
                    memory_max: None,
                    io_weight: None,
                    tmpfs_size: None,
                    proxy: None,
                },
            )]),
            selection: SandboxSelectionConfig::default(),
        };
        let err = ProfileName::Custom("broken".to_string())
            .resolve_profile(&workspace, &config)
            .expect_err("extends=off must Err");
        let msg = err.to_string();
        assert!(
            msg.contains("off") || msg.contains("none"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    #[cfg(all(feature = "enforce", unix))]
    fn resolve_off_returns_err_not_panic() {
        let workspace = std::env::current_dir().unwrap();
        let err = ProfileName::Off
            .resolve_profile(&workspace, &SandboxConfig::default())
            .expect_err("Off.resolve must Err");
        assert!(err.to_string().contains("off"), "unexpected error: {err}");
    }
}
