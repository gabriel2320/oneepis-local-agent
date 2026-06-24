use crate::agent::safety::{sanitize_log, sha256_hex};
use crate::agent::types::{GovernanceDocument, RepoInspection};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const GOVERNANCE_PATHS: &[&str] = &[
    "AGENTS.md",
    "docs/GOVERNANCE.md",
    "docs/CODEX_PLAN.md",
    "docs/CURRENT_STATE.md",
    "docs/SCREEN_TREE.md",
];

pub fn inspect_repository(repo_path: &str) -> Result<RepoInspection, String> {
    let repo = canonical_repo(repo_path)?;
    let is_git_repo = repo.join(".git").exists() || git(&repo, &["rev-parse", "--is-inside-work-tree"]).is_ok();
    let status_text = if is_git_repo {
        git(&repo, &["status", "--short", "--branch"]).unwrap_or_else(|err| err)
    } else {
        "No es un repositorio Git.".to_string()
    };
    let current_branch = if is_git_repo {
        git(&repo, &["branch", "--show-current"])
            .unwrap_or_default()
            .trim()
            .to_string()
    } else {
        String::new()
    };
    let current_branch = if current_branch.is_empty() {
        "detached-or-none".to_string()
    } else {
        current_branch
    };

    let governance_documents = governance_documents(&repo);
    let declared_gates = declared_gates(&repo);
    let is_one_epis = repo.join("AGENTS.md").exists()
        && repo.join("docs").join("GOVERNANCE.md").exists()
        && declared_gates.iter().any(|gate| gate == "check:api");
    let dirty = status_text
        .lines()
        .any(|line| !line.starts_with("##") && !line.trim().is_empty());

    let mut detected_rules = vec![
        "No shell libre generado por IA.".to_string(),
        "No push automatico.".to_string(),
        "No modificar gobernanza para permitir el propio cambio.".to_string(),
    ];
    if is_one_epis {
        detected_rules.extend([
            "OneEpis adapter activo: leer AGENTS.md y docs/GOVERNANCE.md.".to_string(),
            "Preferir paciente/ficha/papel/API/PostgreSQL/auditoria/permisos/OpenAPI.".to_string(),
            "No agregar dashboard, RAG, labs pegados al core, receta, firma ni IA protagonista sin plan explicito.".to_string(),
        ]);
    }

    let mut blocks = Vec::new();
    if !is_git_repo {
        blocks.push("El repo objetivo no es Git; el agente solo opera sobre repos Git.".to_string());
    }
    if dirty {
        blocks.push("Worktree sucio detectado; no se permite aplicar cambios automaticos.".to_string());
    }
    if is_one_epis && !governance_documents.iter().any(|doc| doc.path == "docs/GOVERNANCE.md" && doc.present) {
        blocks.push("OneEpis sin docs/GOVERNANCE.md legible.".to_string());
    }

    Ok(RepoInspection {
        repo_path: repo.display().to_string(),
        project_name: repo
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("repo")
            .to_string(),
        is_git_repo,
        is_one_epis,
        current_branch,
        dirty,
        status_text: sanitize_log(&status_text),
        governance_documents,
        declared_gates,
        detected_rules,
        blocks,
    })
}

fn canonical_repo(repo_path: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(repo_path);
    if !path.exists() {
        return Err(format!("La ruta no existe: {repo_path}"));
    }
    if !path.is_dir() {
        return Err(format!("La ruta no es una carpeta: {repo_path}"));
    }
    path.canonicalize()
        .map_err(|err| format!("No se pudo resolver la ruta: {err}"))
}

fn governance_documents(repo: &Path) -> Vec<GovernanceDocument> {
    GOVERNANCE_PATHS
        .iter()
        .map(|relative| {
            let path = repo.join(relative);
            if !path.exists() {
                return GovernanceDocument {
                    path: relative.to_string(),
                    title: String::new(),
                    sha256: String::new(),
                    bytes: 0,
                    present: false,
                };
            }

            match fs::read(&path) {
                Ok(bytes) => GovernanceDocument {
                    path: relative.to_string(),
                    title: markdown_title(&bytes).unwrap_or_else(|| relative.to_string()),
                    sha256: sha256_hex(&bytes),
                    bytes: bytes.len(),
                    present: true,
                },
                Err(_) => GovernanceDocument {
                    path: relative.to_string(),
                    title: relative.to_string(),
                    sha256: String::new(),
                    bytes: 0,
                    present: false,
                },
            }
        })
        .collect()
}

fn markdown_title(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    text.lines()
        .find_map(|line| line.strip_prefix("# ").map(|title| title.trim().to_string()))
}

fn declared_gates(repo: &Path) -> Vec<String> {
    let package_json = repo.join("package.json");
    let Ok(text) = fs::read_to_string(package_json) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return Vec::new();
    };
    let Some(scripts) = value.get("scripts").and_then(Value::as_object) else {
        return Vec::new();
    };

    let mut gates: Vec<String> = scripts
        .keys()
        .filter(|key| {
            key.as_str() == "check"
                || key.starts_with("check:")
                || key.as_str() == "test"
                || key.as_str() == "build"
        })
        .cloned()
        .collect();
    gates.sort();
    gates
}

pub fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|err| format!("No se pudo ejecutar git: {err}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.success() {
        Ok(stdout.trim_end().to_string())
    } else {
        Err(sanitize_log(stderr.trim()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_repo_is_error() {
        assert!(inspect_repository("Z:\\no-existe-oneepis-local-agent").is_err());
    }
}

