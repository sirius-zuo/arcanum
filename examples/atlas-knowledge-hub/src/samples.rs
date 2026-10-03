//! Sample corpus, golden set, flawed answers and tour data for the Halcyon scenario.

use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleFile {
    pub path: String,
    pub source_uri: String,
    pub title: String,
    pub description: String,
    pub is_update: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenQuery {
    pub query: String,
    pub relevant_source_uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlawedAnswer {
    pub id: String,
    pub title: String,
    pub question: String,
    pub answer: String,
    pub expected: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TourStep {
    pub id: String,
    pub title: String,
    pub why: String,
    pub action: String,
    pub route: String,
    pub completes_when: String,
    pub payoff: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub files: Vec<SampleFile>,
    pub golden: Vec<GoldenQuery>,
    pub flawed_answers: Vec<FlawedAnswer>,
    pub tour: Vec<TourStep>,
}

/// (path, title, description, is_update). The source uri is the file name without directories.
const FILES: [(&str, &str, &str, bool); 11] = [
    (
        "employee-handbook.md",
        "Employee Handbook",
        "Working hours, time off, expenses and safety rules for Halcyon staff.",
        false,
    ),
    (
        "security-policy.md",
        "Security Policy v1",
        "Password rotation every 90 days and optional multi-factor authentication.",
        false,
    ),
    (
        "org-and-teams.md",
        "Organization and Teams",
        "Teams, leads, reporting lines and who owns the Wayfinder navigation stack.",
        false,
    ),
    (
        "incident-2025-03-warehouse-outage.md",
        "Incident Postmortem: March 2025 Outage",
        "A Wayfinder release stalled 212 robots at the Northgate warehouse for 3 hours 40 minutes.",
        false,
    ),
    (
        "product-hx2-datasheet.md",
        "HX-2 Datasheet",
        "Specifications of the current robot: 60 kg payload and 8 hour battery.",
        false,
    ),
    (
        "product-hx1-datasheet.md",
        "HX-1 Datasheet",
        "Specifications of the older robot, with different numbers and a path to the HX-2.",
        false,
    ),
    (
        "customer-faq.md",
        "Customer FAQ",
        "Deployment, operations, data and warranty questions from customers.",
        false,
    ),
    (
        "fleet-ops-runbook.md",
        "Fleet Operations Runbook",
        "Severity levels, current on-call leads per team and incident response steps.",
        false,
    ),
    (
        "roadmap-2026.md",
        "Roadmap 2026",
        "Quarterly engineering plan including the HX-2 Pro and the HX-1 migration.",
        false,
    ),
    (
        "vendor-contracts-summary.md",
        "Vendor Contracts Summary",
        "Main supplier contracts, owners, prices and renewal dates.",
        false,
    ),
    (
        "updates/security-policy.md",
        "Security Policy v2",
        "Update: password rotation every 180 days and mandatory multi-factor authentication.",
        true,
    ),
];

fn resolve_dir(dir: &Path) -> PathBuf {
    match std::env::var_os("ATLAS_SAMPLES_DIR") {
        Some(over) if !over.is_empty() => PathBuf::from(over),
        _ => dir.to_path_buf(),
    }
}

fn read_json<T: serde::de::DeserializeOwned>(dir: &Path, name: &str) -> anyhow::Result<T> {
    let path = dir.join(name);
    let bytes = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parsing {}", path.display()))
}

/// Builds the manifest from the fixed file table and the three JSON files in the samples dir
/// (`ATLAS_SAMPLES_DIR` overrides `dir`).
pub fn load_manifest(dir: &Path) -> anyhow::Result<Manifest> {
    let dir = resolve_dir(dir);
    let files = FILES
        .iter()
        .map(|(path, title, description, is_update)| SampleFile {
            path: (*path).to_string(),
            source_uri: Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            title: (*title).to_string(),
            description: (*description).to_string(),
            is_update: *is_update,
        })
        .collect();
    Ok(Manifest {
        files,
        golden: read_json(&dir, "golden.json")?,
        flawed_answers: read_json(&dir, "flawed-answers.json")?,
        tour: read_json(&dir, "tour.json")?,
    })
}

/// Reads a sample file's bytes; `f.path` is relative to the samples dir.
pub fn read_sample(dir: &Path, f: &SampleFile) -> anyhow::Result<Vec<u8>> {
    let path = resolve_dir(dir).join(&f.path);
    std::fs::read(&path).with_context(|| format!("reading {}", path.display()))
}
