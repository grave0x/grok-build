//! Supply-Chain SBOM Generator.
//!
//! Generates Software Bill of Materials (SBOM) reports in CycloneDX and SPDX
//! formats from Grok Build's dependency graph. Integrates with cargo metadata
//! and external vulnerability databases (NVD, OSV).
//!
//! Spec: modded-featureSpecs/security/02-sbom-generator.md

pub mod format;
pub mod graph;
pub mod scanner;

pub use format::{SbomFormat, export_sbom};
pub use graph::DependencyGraph;
pub use scanner::VulnerabilitySummary;

/// Top-level SBOM document.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SbomDocument {
    pub name: String,
    pub version: String,
    pub generated_at: chrono::DateTime<chrono::Utc>,
    pub components: Vec<ComponentInfo>,
    pub dependencies: Vec<DependencyEdge>,
    pub vulnerabilities: Vec<VulnerabilitySummary>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ComponentInfo {
    pub name: String,
    pub version: String,
    pub purl: Option<String>,
    pub licenses: Vec<String>,
    pub hash_sha256: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DependencyEdge {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, strum::Display)]
pub enum SbomFormat {
    CycloneDxJson,
    SpdxJson,
}

impl SbomFormat {
    pub fn file_extension(&self) -> &'static str {
        match self {
            Self::CycloneDxJson => "cdx.json",
            Self::SpdxJson => "spdx.json",
        }
    }
}

pub fn export_sbom(doc: &SbomDocument, format: SbomFormat) -> anyhow::Result<String> {
    match format {
        SbomFormat::CycloneDxJson | SbomFormat::SpdxJson => {
            serde_json::to_string_pretty(doc).map_err(Into::into)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_sbom_export() {
        let doc = SbomDocument {
            name: "test".into(),
            version: "0.1.0".into(),
            generated_at: chrono::Utc::now(),
            components: vec![],
            dependencies: vec![],
            vulnerabilities: vec![],
        };
        let json = export_sbom(&doc, SbomFormat::CycloneDxJson).unwrap();
        assert!(json.contains("test"));
    }

    #[test]
    fn test_sbom_format_display() {
        assert_eq!(SbomFormat::CycloneDxJson.to_string(), "CycloneDxJson");
        assert_eq!(SbomFormat::SpdxJson.to_string(), "SpdxJson");
    }
}
