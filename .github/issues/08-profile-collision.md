## Project Profile Names Can Collide With Future Global Reserved Names

**Status: RESOLVED**

`built_in_profile_names()` extracted as single source of truth (L92-98). `merge_project_profiles` now checks project profile names against it at load time and emits a `tracing::warn!` when a project defines a profile whose name matches a reserved/built-in name.

Sync test (`built_in_profile_names_matches_parser`) catches drift between the name list and `ProfileName::from_str`. Dedicated test (`merge_project_profiles_warns_on_built_in_name`) confirms the warning fires.

Design choice (deliberate): warning, not hard error. A project using a name that matches a future built-in gets warned at startup but still loads. Making it an error would break existing projects on upgrade. The warning + sync test are adequate for Low severity — the profile name collision is detected at load time, not silently accepted.

**File:** `crates/codegen/xai-grok-sandbox/src/profiles.rs`

**Severity:** Low

### Description

The sandbox profile configuration supports both global (`~/.grok/sandbox.toml`) and project (`.grok/sandbox.toml`) profile definitions. The merge logic correctly prevents project profiles from **overwriting** global profiles via `entry().or_insert()` (L169-173):

```rust
fn merge_project_profiles(config: &mut SandboxConfig, project: SandboxConfig) {
    for (name, profile) in project.profiles {
        config.profiles.entry(name).or_insert(profile);
    }
}
```

However, nothing prevents a project from **defining a custom profile name that matches a future reserved name** or built-in profile. The `ProfileName::from_str` parser (L103-117) treats any unknown name as `Custom(...)`:

```rust
"workspace" => Ok(Self::Workspace),
"devbox" => Ok(Self::Devbox),
"read-only" | "readonly" => Ok(Self::ReadOnly),
"strict" => Ok(Self::Strict),
"off" | "none" => Ok(Self::Off),
other => Ok(Self::Custom(other.to_string())),
```

If a future version adds a new built-in profile (e.g., `"isolated"`) and a project already defines `[profiles.isolated]` locally, the local definition is silently accepted today. After the upgrade, the built-in name takes priority (since `ProfileName::from_str` parses "isolated" as the new built-in variant, not `Custom("isolated")`), and the local profile is **ignored without warning**.

### Scenario

1. Project defines `[profiles.isolated]` in `.grok/sandbox.toml` with custom deny paths
2. User works with this project and relies on the `isolated` profile
3. User upgrades grok to a version that adds a built-in `isolated` profile
4. The project's custom `isolated` is silently dropped in favor of the built-in
5. The deny paths the project relied on are no longer enforced
6. No warning, no error — the profile name resolves to the built-in

### Outcome

- Silent policy degradation after upgrade
- Custom profiles with the same name as newly-added built-ins are dropped without notification
- Security-sensitive deny paths may stop being enforced without the user's knowledge
- Difficult to diagnose: the user sees `profile: isolated` and assumes their custom config is active

### Suggested Fix

1. In `merge_project_profiles`, check if a project profile name matches any known built-in name (including the parse list) and emit a warning
2. On `resolve()` for a `Custom` name, check if the name would parse as a non-Custom variant and warn: "Profile '{name}' shadows a built-in profile name; use a different name for custom profiles"
3. Consider adding a built-in name registry that's checked at config load time
