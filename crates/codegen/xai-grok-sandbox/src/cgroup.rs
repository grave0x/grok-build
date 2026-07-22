//! Cgroup v2 resource limits for sandboxed processes.
//!
//! Creates a child cgroup under the current cgroup, writes CPU/memory/IO
//! limits, and moves the current process into it so all future children
//! inherit the constraints. Gracefully degrades if cgroup v2 is not available.

use std::fs;
use std::io;
use std::path::PathBuf;

fn cgroup_base() -> PathBuf {
    PathBuf::from("/sys/fs/cgroup")
}

fn cgroup_path() -> io::Result<PathBuf> {
    // Read our own cgroup to determine where to create the child.
    // In cgroup v2, /proc/self/cgroup looks like "0::/user.slice/user-1000.slice/session-3.scope"
    let content = fs::read_to_string("/proc/self/cgroup")?;
    let path = content
        .lines()
        .find_map(|l| l.strip_prefix("0::/").map(|p| PathBuf::from(p)))
        .unwrap_or_default();
    Ok(cgroup_base().join(path).join("grok-sandbox"))
}

fn write_checked(path: &std::path::Path, value: &str) -> io::Result<()> {
    fs::write(path, value.as_bytes())
}

/// Setup resource limits by creating a child cgroup and moving the current
/// process into it. All fields are optional — omitted fields are not constrained.
/// Setup resource limits by creating a child cgroup and moving the current
/// process into it. All fields are optional — omitted fields are not constrained.
/// Gracefully degrades (warns, does not hard-fail) when cgroup v2 is unavailable
/// or writes fail.
pub fn setup_limits(
    cpu_quota: Option<&str>,
    memory_max: Option<&str>,
    io_weight: Option<&str>,
) {
    if cpu_quota.is_none() && memory_max.is_none() && io_weight.is_none() {
        return;
    }
    if !cgroup_base().join("cgroup.controllers").exists() {
        tracing::info!("cgroup v2 not available, skipping resource limits");
        return;
    }
    let Ok(path) = cgroup_path() else {
        tracing::warn!("could not determine cgroup path, skipping resource limits");
        return;
    };
    if let Err(e) = fs::create_dir_all(&path) {
        tracing::warn!("failed to create cgroup dir {path:?}: {e}, skipping resource limits");
        return;
    }
    if let Some(quota) = cpu_quota {
        write_checked(&path.join("cpu.max"), quota).unwrap_or_else(|e| {
            tracing::warn!("failed to write cpu.max={quota}: {e}")
        });
    }
    if let Some(max) = memory_max {
        write_checked(&path.join("memory.max"), max).unwrap_or_else(|e| {
            tracing::warn!("failed to write memory.max={max}: {e}")
        });
    }
    if let Some(w) = io_weight {
        write_checked(&path.join("io.weight"), w).unwrap_or_else(|e| {
            tracing::warn!("failed to write io.weight={w}: {e}")
        });
    }
    // Move ourselves into the child cgroup (children inherit limits).
    let pid = std::process::id().to_string();
    write_checked(&path.join("cgroup.procs"), &pid).unwrap_or_else(|e| {
        tracing::warn!("failed to move PID {pid} into cgroup: {e}")
    });
    tracing::info!(pid, path = %path.display(), "moved into resource-limited cgroup");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_checked_sets_content() {
        let dir = std::env::temp_dir().join("cgroup-test-write");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let f = dir.join("test_val");
        write_checked(&f, "hello").unwrap();
        assert_eq!(fs::read_to_string(&f).unwrap(), "hello");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn setup_limits_ok_with_none() {
        setup_limits(None, None, None);
    }

    #[test]
    fn setup_limits_skips_when_v2_not_avail() {
        // cgroup_base()/cgroup.controllers won't exist in temp dirs or non-cgroup systems
        let dir = std::env::temp_dir().join("cgroup-test-notavail");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        // Not actually creating /sys/fs/cgroup/cgroup.controllers — will skip gracefully
        setup_limits(Some("100000 100000"), Some("1G"), Some("100"));
    }
}
