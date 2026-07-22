#![allow(
    unused_imports,
    unused_variables,
    unused_mut,
    unreachable_code,
    dead_code
)]
//! OS-level sandboxing for Grok Build via [nono](https://crates.io/crates/nono).
//!
//! Applied once at process startup. Covers in-process `tokio::fs` calls
//! and child processes. Network is left open at the process level (agent
//! needs LLM API); child network is blocked per-subprocess via seccomp.
//!
//! The `enforce` feature (on by default) pulls in `nono` for
//! kernel-enforced sandboxing (Landlock/Seatbelt). When disabled, the
//! crate still provides lightweight helpers (`log_violation`,
//! `should_restrict_child_network`, `child_net`) that compile on all
//! targets including musl.
//!
//! ```rust,no_run
//! use xai_grok_sandbox::{SandboxManager, ProfileName};
//! use std::path::Path;
//!
//! let workspace = Path::new("/home/user/project");
//! let mut sandbox = SandboxManager::new(ProfileName::Workspace, workspace);
//! sandbox.apply(workspace).expect("sandbox apply failed");
//! sandbox.install();
//! ```
pub mod child_net;
mod cgroup;
mod deny;
mod logging;
mod paths;
mod profiles;
mod types;
pub use logging::SandboxLogger;
#[cfg(all(feature = "enforce", unix))]
use nono::Sandbox;
pub use profiles::{
    ProfileName, SandboxConfig, SandboxProfile, SandboxSelectionConfig, load_sandbox_config,
    sandbox_profile_conflicts,
};
use std::path::Path;
#[cfg(any(target_os = "linux", all(feature = "enforce", test)))]
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
pub use types::{SandboxEvent, SandboxEventType, SandboxMetrics};
static SANDBOX: OnceLock<GlobalSandboxState> = OnceLock::new();
static CONFIGURED_PROFILE: OnceLock<String> = OnceLock::new();
static RESTRICT_CHILD_NETWORK: AtomicBool = AtomicBool::new(false);
static AUTO_ALLOW_BASH: AtomicBool = AtomicBool::new(false);
const BWRAP_ENV_VAR: &str = "__GROK_INSIDE_BWRAP";
pub fn is_inside_bwrap() -> bool {
    std::env::var(BWRAP_ENV_VAR).is_ok()
}
pub fn trust_bwrap_marker_for_devbox() -> bool {
    false
}
struct GlobalSandboxState {
    profile: String,
    logger: SandboxLogger,
    applied: bool,
}
/// Whether child subprocesses should have network blocked via seccomp.
pub fn should_restrict_child_network() -> bool {
    RESTRICT_CHILD_NETWORK.load(Ordering::Relaxed)
}
/// Whether bash commands should be auto-approved when the sandbox is active.
pub fn should_auto_allow_bash() -> bool {
    AUTO_ALLOW_BASH.load(Ordering::Relaxed) && is_active()
}
pub fn set_auto_allow_bash(enabled: bool) {
    AUTO_ALLOW_BASH.store(enabled, Ordering::Relaxed);
}
/// Record the resolved sandbox profile at process startup (including `"off"`).
pub fn set_configured_profile(name: impl Into<String>) {
    let _ = CONFIGURED_PROFILE.set(name.into());
}
/// Resolved sandbox profile from startup, or `None` if `set_configured_profile` was never called.
pub fn configured_profile_name() -> Option<&'static str> {
    CONFIGURED_PROFILE.get().map(|s| s.as_str())
}
/// Whether the sandbox was successfully applied to this process.
pub fn is_active() -> bool {
    SANDBOX.get().is_some_and(|s| s.applied)
}
/// The active sandbox profile name, or `None` if sandbox is not applied.
pub fn profile_name() -> Option<&'static str> {
    SANDBOX
        .get()
        .filter(|s| s.applied)
        .map(|s| s.profile.as_str())
}
/// Log a sandbox violation. Immediately flushed to disk.
/// No-op if sandbox is not active.
pub fn log_violation(target: &str, operation: &str) {
    if let Some(state) = SANDBOX.get() {
        state.logger.log(SandboxEvent::fs_violation(
            &state.profile,
            target,
            operation,
        ));
        let _ = state.logger.flush_to_disk();
    }
}
/// Flush sandbox events to disk. No-op if not initialized.
pub fn flush() {
    if let Some(state) = SANDBOX.get()
        && let Err(e) = state.logger.flush_to_disk()
    {
        tracing::warn!(error = % e, "Failed to flush sandbox events to disk");
    }
}
/// Violation metrics, or `None` if sandbox is not active.
pub fn metrics() -> Option<&'static SandboxMetrics> {
    SANDBOX.get().map(|s| s.logger.metrics())
}
/// Manages the OS-level sandbox. Call `apply()` then `install()`.
pub struct SandboxManager {
    profile: ProfileName,
    logger: SandboxLogger,
    net_restricted: bool,
    network_allow: Vec<String>,
    network_deny: Vec<String>,
    applied: bool,
}
impl SandboxManager {
    /// Create a sandbox manager. Does not apply until `apply()` is called.
    pub fn new(profile: ProfileName, _workspace: &Path) -> Self {
        let net_restricted = profile.restricts_network();
        Self {
            profile,
            logger: SandboxLogger::new(),
            net_restricted,
            network_allow: Vec::new(),
            network_deny: Vec::new(),
            applied: false,
        }
    }
    /// Apply the sandbox to the current process. **Irreversible.**
    /// Degrades gracefully if the platform doesn't support it.
    #[cfg(all(feature = "enforce", unix))]
    pub fn apply(&mut self, workspace: &Path) -> anyhow::Result<()> {
        if self.profile == ProfileName::Off {
            tracing::info!("Sandbox disabled (profile: off)");
            return Ok(());
        }
        let support = Sandbox::support_info();
        if !support.is_supported {
            tracing::warn!(
                details = % support.details,
                "Sandbox not supported on this platform, continuing without sandbox"
            );
            self.logger.log(SandboxEvent::apply_failed(
                &self.profile.to_string(),
                workspace,
                &support.details,
            ));
            return Ok(());
        }
        let config = profiles::load_sandbox_config(workspace);
        let caps = self
            .profile
            .to_capability_set_with_config(workspace, &config)?;
        let mut resolved = self.profile.resolve_profile(workspace, &config)?;
        resolved.deny = deny::effective_deny_paths(workspace, &resolved.deny);
        cgroup::setup_limits(
            resolved.cpu_quota.as_deref(),
            resolved.memory_max.as_deref(),
            resolved.io_weight.as_deref(),
        );
        self.net_restricted = self.profile.restricts_network_resolved(&config);
        self.network_allow = resolved.network_allow.clone();
        self.network_deny = resolved.network_deny.clone();
        if !self.network_allow.is_empty() || !self.network_deny.is_empty() {
            self.net_restricted = true;
        }
        match Sandbox::apply(&caps) {
            Ok(_) => {
                self.applied = true;
                if self.net_restricted {
                    RESTRICT_CHILD_NETWORK.store(true, Ordering::Relaxed);
                }
                self.logger.log(SandboxEvent::profile_applied(
                    &self.profile.to_string(),
                    workspace,
                    &resolved,
                ));
                tracing::info!(
                    profile = % self.profile, workspace = % workspace.display(),
                    restrict_network = self.net_restricted,
                    "Sandbox applied (kernel-enforced, irreversible)"
                );
                Ok(())
            }
            Err(e) => {
                tracing::warn!(
                    profile = % self.profile, error = % e,
                    "Sandbox could not be applied, continuing without sandbox"
                );
                self.logger.log(SandboxEvent::apply_failed(
                    &self.profile.to_string(),
                    workspace,
                    &e,
                ));
                Ok(())
            }
        }
    }
    /// Stub when `enforce` feature is disabled — sandbox is not applied.
    #[cfg(not(all(feature = "enforce", unix)))]
    pub fn apply(&mut self, _workspace: &Path) -> anyhow::Result<()> {
        tracing::info!(
            profile = % self.profile,
            "Sandbox enforcement unavailable (built without 'enforce' feature)"
        );
        Ok(())
    }
    /// Store globally for session-lifetime violation logging.
    pub fn install(self) {
        let _ = self.logger.flush_to_disk();
        let _ = SANDBOX.set(GlobalSandboxState {
            profile: self.profile.to_string(),
            logger: self.logger,
            applied: self.applied,
        });
    }
    /// Check whether the current platform supports sandboxing.
    #[cfg(all(feature = "enforce", unix))]
    pub fn support_info() -> nono::SupportInfo {
        Sandbox::support_info()
    }
    /// Whether the sandbox was successfully applied.
    pub fn is_applied(&self) -> bool {
        self.applied
    }
    /// Whether child subprocesses should have network blocked.
    pub fn restrict_child_network(&self) -> bool {
        self.applied && self.net_restricted
    }
    /// Hostnames/IPs allowed for child network (empty = no allowlist).
    pub fn network_allow(&self) -> &[String] {
        &self.network_allow
    }
    /// Hostnames/IPs denied for child network (empty = no denylist).
    pub fn network_deny(&self) -> &[String] {
        &self.network_deny
    }
    /// The active profile name.
    pub fn profile(&self) -> &ProfileName {
        &self.profile
    }
    /// Access the sandbox event logger (before `install()`).
    pub fn logger(&self) -> &SandboxLogger {
        &self.logger
    }
}
/// Build a bwrap command that re-execs the current process with
/// `deny_write` paths mounted read-only and `deny_read` paths bound
/// over with an unreadable placeholder (EPERM on read).
///
/// Returns `None` if already inside bwrap. Caller should `cmd.exec()` the result.
pub fn bwrap_reexec_command(
    deny_write: &[&str],
    deny_read: &[&str],
    tmpfs_size: Option<&str>,
) -> Option<std::process::Command> {
    if is_inside_bwrap() {
        return None;
    }
    // Verify bwrap is installed before constructing the command.
    if std::process::Command::new("bwrap")
        .arg("--version")
        .output()
        .is_err()
    {
        tracing::error!("bwrap is required for sandbox read-deny enforcement but is not installed");
        return None;
    }
    let self_exe = std::env::current_exe().ok()?;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut cmd = std::process::Command::new("bwrap");
    cmd.arg("--bind").arg("/").arg("/");
    for path in deny_write {
        if Path::new(path).exists() {
            cmd.arg("--ro-bind").arg(path).arg(path);
        }
    }
    #[cfg(target_os = "linux")]
    if !deny_read.is_empty() {
        for path in deny_read {
            let Some(blocked) = bwrap_blocked_source_for_path(Path::new(path)) else {
                tracing::error!(
                    path,
                    "could not create bwrap placeholder for read-deny path; \
                     refusing to start with a partial sandbox"
                );
                return None;
            };
            cmd.arg("--ro-bind").arg(&blocked).arg(path);
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = deny_read;
    cmd.arg("--dev-bind").arg("/dev").arg("/dev");
    cmd.arg("--proc").arg("/proc");
    if let Some(size) = tmpfs_size {
        cmd.arg("--tmpfs").arg(format!("/tmp:size={size}"));
    }
    cmd.env(BWRAP_ENV_VAR, "1");
    cmd.arg("--").arg(self_exe).args(args);
    Some(cmd)
}
/// Choose file vs directory placeholder for a deny path (existing dirs need a dir bind).
#[cfg(all(feature = "enforce", target_os = "linux"))]
fn bwrap_blocked_source_for_path(path: &Path) -> Option<PathBuf> {
    if deny::deny_path_is_dir(path) {
        bwrap_blocked_placeholder("sandbox-blocked-dir", true)
    } else {
        bwrap_blocked_placeholder("sandbox-blocked", false)
    }
}
/// Without kernel enforcement there are no read-deny placeholders to bind over.
#[cfg(all(not(feature = "enforce"), target_os = "linux"))]
fn bwrap_blocked_source_for_path(_path: &Path) -> Option<PathBuf> {
    None
}
/// chmod a placeholder to mode 000 so a bwrap bind-over yields EPERM on read.
#[cfg(all(feature = "enforce", target_os = "linux"))]
fn chmod_000(path: &Path) -> Option<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path).ok()?.permissions();
    perms.set_mode(0o000);
    std::fs::set_permissions(path, perms).ok()?;
    Some(())
}
/// Zero-permission placeholder (file or dir) under `grok_home` used by bwrap bind-over.
///
/// The placeholder name is suffixed with the current PID so concurrent grok
/// processes don't race each other's create/remove/chmod on a shared path (which
/// could yield `None` and the silent dropped-bind fail-open this avoids).
#[cfg(all(feature = "enforce", target_os = "linux"))]
fn bwrap_blocked_placeholder(name: &str, want_dir: bool) -> Option<PathBuf> {
    use std::fs::OpenOptions;
    let path = paths::grok_home().join(format!("{name}.{}", std::process::id()));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok()?;
    }
    if path.exists() {
        if path.is_dir() == want_dir {
            chmod_000(&path)?;
            return Some(path);
        }
        if path.is_dir() {
            std::fs::remove_dir_all(&path).ok()?;
        } else {
            std::fs::remove_file(&path).ok()?;
        }
    }
    if want_dir {
        std::fs::create_dir(&path).ok()?;
    } else {
        OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&path)
            .ok()?;
    }
    chmod_000(&path)?;
    Some(path)
}
/// Whether a profile write-denies `/data` via the devbox bwrap bind (built-in
/// `devbox` or a custom profile that `extends = "devbox"`). This is a pure mount,
/// so it applies even WITHOUT the `enforce` feature.
///
/// On non-Linux platforms the `/data` write-deny is best-effort — warn once.
fn is_devbox_based(profile: &ProfileName, config: &SandboxConfig) -> bool {
    let result = match profile {
        ProfileName::Devbox => true,
        ProfileName::Custom(name) => {
            config.profiles.get(name).and_then(|p| p.extends.as_deref()) == Some("devbox")
        }
        _ => false,
    };
    if result {
        #[cfg(not(target_os = "linux"))]
        {
            static WARNED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
            WARNED.get_or_init(|| {
                tracing::warn!(
                    "Devbox /data write-deny requires bwrap (Linux only); \
                     /data will be writable on this platform"
                );
                true
            });
        }
    }
    result
}
/// Whether kernel read-deny enforcement is required. The single source of truth
/// for this classification so callers (e.g. the shell's fail-closed startup path)
/// cannot drift and silently fail open.
///
/// Decided directly from the profile config (a `Custom` profile with a non-empty
/// `deny`) — NOT from the resolved/expanded deny set, which returns empty on
/// failure. Keying "requires" on that empty-on-error result would silently
/// downgrade to fail-open (Linux) when resolution hiccups; this intrinsic check
/// stays fail-closed.
#[cfg(all(feature = "enforce", unix))]
pub fn requires_read_deny(profile: &ProfileName, workspace: &Path) -> bool {
    match profile {
        ProfileName::Custom(name) => {
            let config = profiles::load_sandbox_config(workspace);
            config
                .profiles
                .get(name)
                .is_some_and(|p| !p.deny.is_empty())
        }
        _ => false,
    }
}
/// Stub when `enforce` is unavailable — nothing is kernel-enforced.
#[cfg(not(all(feature = "enforce", unix)))]
pub fn requires_read_deny(_profile: &ProfileName, _workspace: &Path) -> bool {
    false
}
/// A profile's resolved bwrap deny plan: read-only mounts (`deny_write`),
/// bound-over unreadable placeholders (`deny_read`), and whether the profile
/// carries deny globs (`has_globs`, so the re-exec proceeds even with zero
/// current matches — globs are best-effort on Linux).
#[cfg(target_os = "linux")]
struct BwrapDenyPlan {
    deny_write: Vec<String>,
    deny_read: Vec<String>,
    has_globs: bool,
}
/// Resolve a profile's full [`BwrapDenyPlan`] in ONE config read: the `/data`
/// write-deny (devbox and devbox-extending customs), the exact read-deny paths,
/// and the launch-time glob expansion. Returns `None` (fail closed) if a deny
/// glob blows past the expansion caps or is invalid, so
/// [`bwrap_reexec_for_profile`] refuses to start.
///
/// Best-effort on Linux: a mount namespace can't glob at runtime, so globs are
/// expanded once here at launch — files matching them that are created LATER are
/// NOT covered (macOS Seatbelt enforces the same globs as runtime regexes).
#[cfg(all(feature = "enforce", target_os = "linux"))]
fn bwrap_deny_plan(profile: &ProfileName, workspace: &Path) -> Option<BwrapDenyPlan> {
    let config = profiles::load_sandbox_config(workspace);
    let deny_write: Vec<String> = if is_devbox_based(profile, &config) {
        vec!["/data".to_string()]
    } else {
        Vec::new()
    };
    let entries = if *profile == ProfileName::Off {
        Vec::new()
    } else {
        profile
            .resolve_profile(workspace, &config)
            .map(|r| r.deny)
            .unwrap_or_default()
    };
    let (exact, globs) = deny::partition_deny_entries(&entries);
    let mut deny_read = deny::exact_deny_path_strings(workspace, &exact);
    let has_globs = !globs.is_empty();
    if has_globs {
        tracing::warn!(
            count = globs.len(),
            "sandbox deny globs are enforced best-effort on Linux (expanded at launch); \
             files matching them that are created later are NOT covered"
        );
        deny_read.extend(deny::expand_deny_globs(
            workspace,
            &globs,
            deny::DENY_GLOB_MAX_DEPTH,
            deny::DENY_GLOB_MAX_MATCHES,
            deny::DENY_GLOB_MAX_ENTRIES,
        )?);
    }
    Some(BwrapDenyPlan {
        deny_write,
        deny_read,
        has_globs,
    })
}
/// Stub when `enforce` is unavailable on Linux: read-deny needs nono, so there is
/// none — but the devbox `/data` write-deny is a plain bwrap mount and MUST still
/// apply (devbox `/data` is always sandboxed), so it is preserved here.
#[cfg(all(not(feature = "enforce"), target_os = "linux"))]
fn bwrap_deny_plan(profile: &ProfileName, workspace: &Path) -> Option<BwrapDenyPlan> {
    let config = profiles::load_sandbox_config(workspace);
    let deny_write: Vec<String> = if is_devbox_based(profile, &config) {
        vec!["/data".to_string()]
    } else {
        Vec::new()
    };
    Some(BwrapDenyPlan {
        deny_write,
        deny_read: Vec::new(),
        has_globs: false,
    })
}
/// Build the bwrap re-exec command needed on Linux, or `None` if no mount-namespace
/// enforcement is needed (or we are already inside bwrap). Canonical routing:
/// devbox — and a custom profile that `extends = "devbox"` — gets write-deny on
/// `/data`; any profile gets read-deny on its own `deny` set. These compose, so a
/// devbox-based custom profile with a `deny` list write-denies `/data` AND
/// read-denies its deny paths in one re-exec.
///
/// Glob deny entries are expanded to concrete existing matches at launch and
/// bound over too (best-effort; post-launch matches are not covered on Linux).
/// Returns `None` (fail closed) if a glob blows past the expansion caps, so the
/// shell's startup refuses to run with a broad glob under-enforced.
#[cfg(target_os = "linux")]
pub fn bwrap_reexec_for_profile(
    profile: &ProfileName,
    workspace: &Path,
) -> Option<std::process::Command> {
    let BwrapDenyPlan {
        deny_write,
        deny_read,
        has_globs,
    } = bwrap_deny_plan(profile, workspace)?;
    let config = profiles::load_sandbox_config(workspace);
    let tmpfs_size = if *profile != ProfileName::Off {
        profile
            .resolve_profile(workspace, &config)
            .ok()
            .and_then(|r| r.tmpfs_size.clone())
    } else {
        None
    };
    if deny_write.is_empty() && deny_read.is_empty() && !has_globs && tmpfs_size.is_none() {
        return None;
    }
    let write_refs: Vec<&str> = deny_write.iter().map(String::as_str).collect();
    let read_refs: Vec<&str> = deny_read.iter().map(String::as_str).collect();
    bwrap_reexec_command(&write_refs, &read_refs, tmpfs_size.as_deref())
}
#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    /// Save, set/remove, and auto-restore an env var on drop.
    struct EnvGuard {
        key: &'static str,
        prev: Option<String>,
    }
    impl EnvGuard {
        fn set(key: &'static str, val: &str) -> Self {
            let prev = std::env::var(key).ok();
            unsafe { std::env::set_var(key, val) };
            Self { key, prev }
        }
        fn remove(key: &'static str) -> Self {
            let prev = std::env::var(key).ok();
            unsafe { std::env::remove_var(key) };
            Self { key, prev }
        }
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.prev {
                Some(v) => unsafe { std::env::set_var(self.key, v) },
                None => unsafe { std::env::remove_var(self.key) },
            }
        }
    }
    #[test]
    #[serial(bwrap_env)]
    fn bwrap_reexec_returns_none_inside_bwrap() {
        let _g = EnvGuard::set(BWRAP_ENV_VAR, "1");
        let result = bwrap_reexec_command(&["/data"], &[], None);
        assert!(
            result.is_none(),
            "should return None when already inside bwrap"
        );
    }
    #[test]
    #[serial(bwrap_env)]
    fn bwrap_reexec_returns_some_outside_bwrap() {
        let _g = EnvGuard::remove(BWRAP_ENV_VAR);
        let result = bwrap_reexec_command(&["/tmp"], &[], None);
        assert!(result.is_some(), "should return Some when not inside bwrap");
        let cmd = result.unwrap();
        assert_eq!(cmd.get_program(), "bwrap", "program should be bwrap");
    }
    #[test]
    #[serial(bwrap_env)]
    fn trust_bwrap_marker_for_devbox_tracks_env_when_feature_on() {
        let _g = EnvGuard::set(BWRAP_ENV_VAR, "1");
        assert!(
            !trust_bwrap_marker_for_devbox(),
            "without bwrap-marker the hatch must stay closed"
        );
    }
    #[test]
    #[serial(bwrap_env)]
    fn trust_bwrap_marker_for_devbox_false_outside_bwrap() {
        let _g = EnvGuard::remove(BWRAP_ENV_VAR);
        assert!(!trust_bwrap_marker_for_devbox());
        assert!(!is_inside_bwrap());
    }
    #[test]
    #[serial(bwrap_env)]
    fn bwrap_reexec_skips_nonexistent_paths() {
        let _g = EnvGuard::remove(BWRAP_ENV_VAR);
        let result = bwrap_reexec_command(&["/nonexistent-test-path-xyz-12345"], &[], None);
        let cmd = result.unwrap();
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert!(
            !args.iter().any(|a| a == "/nonexistent-test-path-xyz-12345"),
            "should skip non-existent deny_write paths, got args: {args:?}"
        );
    }
    #[test]
    #[serial(bwrap_env)]
    #[cfg(all(feature = "enforce", target_os = "linux"))]
    fn bwrap_reexec_binds_nonexistent_deny_read_paths() {
        let _g = EnvGuard::remove(BWRAP_ENV_VAR);
        let missing = "/nonexistent-deny-read-path-xyz-12345";
        let result = bwrap_reexec_command(&[], &[missing], None);
        let cmd = result.unwrap();
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        let has_bind = args
            .windows(3)
            .any(|w| w[0] == "--ro-bind" && w[2] == missing);
        assert!(
            has_bind,
            "should bind-over non-existent deny_read paths, got args: {args:?}"
        );
    }
    #[test]
    #[serial(bwrap_env)]
    fn bwrap_reexec_mounts_existing_paths_read_only() {
        let _g = EnvGuard::remove(BWRAP_ENV_VAR);
        let result = bwrap_reexec_command(&["/tmp"], &[], None);
        let cmd = result.unwrap();
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        let has_ro_bind = args.windows(3).any(|w| w == ["--ro-bind", "/tmp", "/tmp"]);
        assert!(
            has_ro_bind,
            "should mount existing paths as --ro-bind, got args: {args:?}"
        );
    }
    #[test]
    #[serial(bwrap_env)]
    fn bwrap_reexec_uses_dev_bind() {
        let _g = EnvGuard::remove(BWRAP_ENV_VAR);
        let result = bwrap_reexec_command(&[], &[], None);
        let cmd = result.unwrap();
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        let has_dev_bind = args.windows(3).any(|w| w == ["--dev-bind", "/dev", "/dev"]);
        assert!(
            has_dev_bind,
            "should use --dev-bind for /dev passthrough, got args: {args:?}"
        );
    }
    #[test]
    fn configured_profile_is_recorded() {
        set_configured_profile("read-only");
        assert_eq!(configured_profile_name(), Some("read-only"));
    }
    /// Create a temp workspace whose `.grok/sandbox.toml` contains `toml_body`.
    /// Returns the workspace path (caller removes it).
    #[cfg(all(feature = "enforce", unix))]
    fn temp_workspace_with_sandbox_toml(tag: &str, toml_body: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let ws = std::env::temp_dir().join(format!("grok-{tag}-{}-{nanos}", std::process::id()));
        let grok = ws.join(".grok");
        std::fs::create_dir_all(&grok).unwrap();
        std::fs::write(grok.join("sandbox.toml"), toml_body).unwrap();
        ws
    }
    /// Create a temp workspace defining a `denytest` profile (extends `workspace`)
    /// with the given `deny` list. `deny_toml` is the raw TOML array body
    /// (e.g. `"\".env\""`).
    #[cfg(all(feature = "enforce", unix))]
    fn temp_workspace_with_deny(tag: &str, deny_toml: &str) -> PathBuf {
        temp_workspace_with_sandbox_toml(
            tag,
            &format!("[profiles.denytest]\nextends = \"workspace\"\ndeny = [{deny_toml}]\n"),
        )
    }
    #[test]
    #[cfg(all(feature = "enforce", unix))]
    fn requires_read_deny_only_for_custom_profile_with_deny() {
        let ws = temp_workspace_with_deny("requires-deny", "\".env\"");
        assert!(requires_read_deny(
            &ProfileName::Custom("denytest".to_string()),
            &ws
        ));
        assert!(!requires_read_deny(
            &ProfileName::Custom("undefined".to_string()),
            &ws
        ));
        assert!(!requires_read_deny(&ProfileName::Workspace, &ws));
        assert!(!requires_read_deny(&ProfileName::Strict, &ws));
        assert!(!requires_read_deny(&ProfileName::Devbox, &ws));
        assert!(!requires_read_deny(&ProfileName::Off, &ws));
        let _ = std::fs::remove_dir_all(&ws);
    }
    #[test]
    #[serial(bwrap_env)]
    #[cfg(all(feature = "enforce", target_os = "linux"))]
    fn bwrap_reexec_uses_dir_placeholder_for_directories() {
        let _g = EnvGuard::remove(BWRAP_ENV_VAR);
        let dir = std::env::temp_dir().join(format!("grok-deny-dir-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dir_str = dir.to_string_lossy().to_string();
        let result = bwrap_reexec_command(&[], &[&dir_str], None);
        let cmd = result.unwrap();
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        let blocked_dir = paths::grok_home()
            .join(format!("sandbox-blocked-dir.{}", std::process::id()))
            .to_string_lossy()
            .to_string();
        let has_dir_bind = args
            .windows(3)
            .any(|w| w[0] == "--ro-bind" && w[1] == blocked_dir && w[2] == dir_str);
        assert!(
            has_dir_bind,
            "existing directories should bind over sandbox-blocked-dir, got args: {args:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    #[serial(bwrap_env)]
    #[cfg(all(feature = "enforce", target_os = "linux"))]
    fn bwrap_reexec_for_profile_devbox_extends_composes_data_and_read_deny() {
        let _g = EnvGuard::remove(BWRAP_ENV_VAR);
        let ws = temp_workspace_with_sandbox_toml(
            "devbox-compose",
            "[profiles.devcustom]\nextends = \"devbox\"\ndeny = [\"secret.pem\"]\n",
        );
        let cmd = bwrap_reexec_for_profile(&ProfileName::Custom("devcustom".to_string()), &ws)
            .expect("devbox-extending custom with deny should build a re-exec command");
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        let deny_path = ws.join("secret.pem").to_string_lossy().to_string();
        assert!(
            args.windows(3)
                .any(|w| w[0] == "--ro-bind" && w[2] == deny_path),
            "expected read-deny bind for {deny_path}, got args: {args:?}"
        );
        if Path::new("/data").exists() {
            assert!(
                args.windows(3)
                    .any(|w| w == ["--ro-bind", "/data", "/data"]),
                "expected /data write-deny ro-bind, got args: {args:?}"
            );
        }
        let _ = std::fs::remove_dir_all(&ws);
        let ws_empty = temp_workspace_with_sandbox_toml(
            "devbox-empty",
            "[profiles.devempty]\nextends = \"devbox\"\n",
        );
        assert!(
            bwrap_reexec_for_profile(&ProfileName::Custom("devempty".to_string()), &ws_empty)
                .is_some(),
            "devbox-extending custom must compose the /data write-deny re-exec"
        );
        let _ = std::fs::remove_dir_all(&ws_empty);
        let ws_ws = temp_workspace_with_sandbox_toml(
            "ws-empty",
            "[profiles.wsempty]\nextends = \"workspace\"\n",
        );
        assert!(
            bwrap_reexec_for_profile(&ProfileName::Custom("wsempty".to_string()), &ws_ws).is_none(),
            "non-devbox custom with no deny needs no re-exec"
        );
        let _ = std::fs::remove_dir_all(&ws_ws);
    }
    #[test]
    #[serial(bwrap_env)]
    fn bwrap_reexec_appends_tmpfs_when_size_given() {
        let _g = EnvGuard::remove(BWRAP_ENV_VAR);
        let result = bwrap_reexec_command(&[], &[], Some("512M"));
        let cmd = result.unwrap();
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert!(
            args.windows(2).any(|w| w[0] == "--tmpfs" && w[1] == "/tmp:size=512M"),
            "expected --tmpfs /tmp:size=512M, got args: {args:?}"
        );
    }
    #[test]
    #[serial(bwrap_env)]
    fn bwrap_reexec_no_tmpfs_when_size_none() {
        let _g = EnvGuard::remove(BWRAP_ENV_VAR);
        let result = bwrap_reexec_command(&[], &[], None);
        let cmd = result.unwrap();
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert!(
            !args.contains(&"--tmpfs".to_string()),
            "should not include --tmpfs when size is None, got args: {args:?}"
        );
    }
    #[test]
    #[serial(bwrap_env)]
    #[cfg(all(feature = "enforce", unix))]
    fn bwrap_reexec_for_profile_triggers_on_tmpfs_size_alone() {
        let _g = EnvGuard::remove(BWRAP_ENV_VAR);
        let ws = temp_workspace_with_sandbox_toml(
            "tmpfs-size",
            "[profiles.custom-tmpfs]\nextends = \"workspace\"\ntmpfs_size = \"256M\"\n",
        );
        let cmd = bwrap_reexec_for_profile(
            &ProfileName::Custom("custom-tmpfs".to_string()),
            &ws,
        );
        assert!(
            cmd.is_some(),
            "profile with tmpfs_size alone should trigger re-exec"
        );
        let args: Vec<String> = cmd
            .unwrap()
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert!(
            args.windows(2).any(|w| w[0] == "--tmpfs" && w[1] == "/tmp:size=256M"),
            "expected --tmpfs /tmp:size=256M, got args: {args:?}"
        );
        let _ = std::fs::remove_dir_all(&ws);
    }
    #[test]
    #[serial(bwrap_env)]
    #[cfg(all(feature = "enforce", unix))]
    fn bwrap_reexec_for_profile_skips_tmpfs_on_off() {
        let result = bwrap_reexec_for_profile(&ProfileName::Off, &std::env::temp_dir());
        assert!(
            result.is_none(),
            "Off profile should not trigger re-exec"
        );
    }
    #[test]
    #[cfg(all(feature = "enforce", unix))]
    fn sandbox_manager_stores_network_lists() {
        let ws = temp_workspace_with_sandbox_toml(
            "netlist",
            "[profiles.netlist]\nextends = \"workspace\"\n\
             network_allow = [\"api.example.com\"]\n\
             network_deny = [\"evil.example.com\"]\n",
        );
        let mut manager = SandboxManager::new(
            ProfileName::Custom("netlist".to_string()),
            &ws,
        );
        let _ = manager.apply(&ws);
        // When apply resolves the profile (supported platform), lists are populated.
        // On unsupported platforms they remain empty — both outcomes are valid.
        if manager.is_applied() {
            assert_eq!(manager.network_allow(), &["api.example.com"]);
            assert_eq!(manager.network_deny(), &["evil.example.com"]);
        } else {
            assert!(manager.network_allow().is_empty());
            assert!(manager.network_deny().is_empty());
        }
        let _ = std::fs::remove_dir_all(&ws);
    }
    #[test]
    #[cfg(all(feature = "enforce", unix))]
    fn sandbox_manager_network_allow_triggers_restrict() {
        let ws = temp_workspace_with_sandbox_toml(
            "net-allow-only",
            "[profiles.netallowonly]\nextends = \"workspace\"\n\
             network_allow = [\"api.example.com\"]\n",
        );
        let mut manager = SandboxManager::new(
            ProfileName::Custom("netallowonly".to_string()),
            &ws,
        );
        let _ = manager.apply(&ws);
        if manager.is_applied() {
            assert!(manager.restrict_child_network(),
                "network_allow should force restrict_child_network to true when applied");
        }
        let _ = std::fs::remove_dir_all(&ws);
    }
}
