//! Audit export formats: JSON, CEF (Common Event Format), and CSV.

use crate::entry::AuditEntry;

/// Supported export formats.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ExportFormat {
    /// Newline-delimited JSON (one entry per line).
    Json,
    /// Common Event Format (CEF) for SIEM ingestion.
    Cef,
    /// Comma-Separated Values.
    Csv,
}

/// Export a slice of audit entries in the given format.
pub fn export(entries: &[AuditEntry], format: ExportFormat) -> String {
    match format {
        ExportFormat::Json => export_json(entries),
        ExportFormat::Cef => export_cef(entries),
        ExportFormat::Csv => export_csv(entries),
    }
}

fn export_json(entries: &[AuditEntry]) -> String {
    entries
        .iter()
        .map(|e| serde_json::to_string(e).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n")
}

/// CEF format: CEF:Version|Device Vendor|Device Product|Device Version|Signature ID|Name|Severity|Extension
fn export_cef(entries: &[AuditEntry]) -> String {
    entries
        .iter()
        .map(|e| {
            format!(
                "CEF:0|xAI|GrokBuild|1.0|{}|{}|{}|msg={} src={} suser={} start={} outcome={}",
                match e.event_type {
                    crate::AuditEventType::SandboxViolation
                    | crate::AuditEventType::PermissionDenied => 100,
                    _ => 200,
                },
                e.tool_name.as_deref().unwrap_or("Unknown"),
                match e.exit_code {
                    Some(0) | None => 0, // Informational or success
                    Some(_) => 7,         // Error
                },
                e.result_summary
                    .as_deref()
                    .unwrap_or("")
                    .replace('|', "\\|")
                    .replace('\n', " "),
                e.cwd,
                e.session_id,
                e.timestamp.to_rfc3339(),
                match e.exit_code {
                    Some(0) => "Success",
                    None => "Unknown",
                    Some(_) => "Failure",
                },
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn export_csv(entries: &[AuditEntry]) -> String {
    let mut out = String::from(
        "id,session_id,timestamp,event_type,tool_name,exit_code,duration_ms,cwd,summary\n",
    );
    for e in entries {
        out.push_str(&format!(
            "{},{},{},{},{},{:?},{:?},{},\"{}\"\n",
            e.id.unwrap_or(0),
            e.session_id,
            e.timestamp.to_rfc3339(),
            e.event_type,
            e.tool_name.as_deref().unwrap_or(""),
            e.exit_code.map(|c| c.to_string()).unwrap_or_default(),
            e.duration_ms.map(|d| d.to_string()).unwrap_or_default(),
            e.cwd,
            e.result_summary
                .as_deref()
                .unwrap_or("")
                .replace('"', "\"\""),
        ));
    }
    out
}
