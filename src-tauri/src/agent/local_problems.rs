use crate::agent::gates::run_gate;
use crate::agent::repo::{canonical_repo, git, inspect_repository};
use crate::agent::safety::{sanitize_log, sha256_hex};
use crate::agent::types::{
    LocalProblemPlan, LocalProblemRequest, LocalProblemRun, LocalProblemSpec,
};
use chrono::Utc;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;

const GLOBAL_FORBIDDEN_SIGNALS: &[&str] = &[
    "endpoint",
    "new route",
    "ruta nueva",
    "permission",
    "permiso",
    "migration",
    "migracion",
    "alembic",
    "rag",
    "dashboard",
    "receta",
    "prescription",
    "firma",
    "signature",
    "external ai",
    "openai",
    "anthropic",
];

pub fn list_local_problems() -> Vec<LocalProblemSpec> {
    local_problem_specs()
}

pub fn local_problem_plan(request: LocalProblemRequest) -> Result<LocalProblemPlan, String> {
    let problem = local_problem_spec(&request.problem_id)?;
    let inspection = inspect_repository(&request.repo_path)?;
    let mut blockers = Vec::new();
    let mut warnings = Vec::new();

    if !inspection.is_git_repo {
        blockers.push("El proyecto objetivo debe ser un repo Git.".to_string());
    }
    if !inspection.is_one_epis {
        blockers
            .push("El adaptador OneEpis debe estar activo antes de ejecutar LOCAL-*.".to_string());
    }
    for gate in &problem.gates {
        if !inspection.declared_gates.contains(gate) {
            blockers.push(format!("Falta gate declarado en package.json: {gate}."));
        }
    }
    if inspection.dirty && inspection.current_branch != problem.branch {
        blockers.push(format!(
            "Hay cambios pendientes fuera de la rama segura {}; primero guarda o descarta esos cambios.",
            problem.branch
        ));
    } else if inspection.dirty {
        warnings.push(
            "Hay cambios pendientes en la rama segura; solo se podran commitear si pasan validacion LOCAL."
                .to_string(),
        );
    }

    let status = if blockers.is_empty() {
        "ready"
    } else {
        "blocked"
    };
    let next_actions = if blockers.is_empty() {
        vec![
            format!("Preparar rama local con LOCAL {}.", problem.id),
            "Ejecutar el cambio pequeno permitido por la ficha LOCAL.".to_string(),
            "Correr gates y crear commit local sin push automatico.".to_string(),
        ]
    } else {
        blockers
            .iter()
            .take(3)
            .cloned()
            .chain(["No ejecutar cambios hasta resolver bloqueos.".to_string()])
            .collect()
    };

    Ok(LocalProblemPlan {
        repo_path: inspection.repo_path,
        problem,
        status: status.to_string(),
        blockers,
        warnings,
        next_actions,
        no_push: true,
    })
}

pub async fn prepare_local_problem(
    request: LocalProblemRequest,
) -> Result<LocalProblemRun, String> {
    let problem = local_problem_spec(&request.problem_id)?;
    let inspection = inspect_repository(&request.repo_path)?;
    let repo = canonical_repo(&request.repo_path)?;
    let mut blockers = common_blocks(&inspection, &problem);

    if inspection.dirty {
        blockers.push("La preparacion de rama requiere proyecto limpio.".to_string());
    }
    if !blockers.is_empty() {
        return Ok(blocked_run(&inspection.repo_path, &problem, blockers));
    }

    let branch = ensure_problem_branch(&repo, &inspection.current_branch, &problem.branch)?;
    Ok(LocalProblemRun {
        id: run_id(&inspection.repo_path, &problem.id),
        problem_id: problem.id.clone(),
        status: "ready_for_changes".to_string(),
        repo_path: inspection.repo_path,
        branch,
        commit_sha: None,
        changed_files: Vec::new(),
        gate_results: Vec::new(),
        blockers: Vec::new(),
        warnings: vec!["No hay push automatico; el ciclo termina en commit local.".to_string()],
        next_actions: vec![
            "Realizar solo el refactor permitido por esta ficha LOCAL.".to_string(),
            "Ejecutar local-problem-commit para validar gates y crear commit local.".to_string(),
        ],
        no_push: true,
        summary: format!("Rama segura preparada para {}.", problem.id),
    })
}

pub async fn commit_local_problem(request: LocalProblemRequest) -> Result<LocalProblemRun, String> {
    let problem = local_problem_spec(&request.problem_id)?;
    let inspection = inspect_repository(&request.repo_path)?;
    let repo = canonical_repo(&request.repo_path)?;
    let mut blockers = common_blocks(&inspection, &problem);

    if inspection.current_branch != problem.branch {
        blockers.push(format!(
            "El commit LOCAL debe hacerse en rama segura {}; rama actual: {}.",
            problem.branch, inspection.current_branch
        ));
    }

    let changed_files = changed_files(&repo)?;
    if changed_files.is_empty() {
        blockers.push("No hay cambios para commitear.".to_string());
    }

    blockers.extend(validate_changed_files(&problem, &changed_files));
    blockers.extend(validate_diff_content(&repo, &problem, &changed_files)?);

    if !blockers.is_empty() {
        return Ok(LocalProblemRun {
            id: run_id(&inspection.repo_path, &problem.id),
            problem_id: problem.id.clone(),
            status: "blocked".to_string(),
            repo_path: inspection.repo_path,
            branch: inspection.current_branch,
            commit_sha: None,
            changed_files,
            gate_results: Vec::new(),
            blockers,
            warnings: Vec::new(),
            next_actions: vec![
                "Reducir el cambio a la ficha LOCAL y volver a validar.".to_string(),
                "No crear commit hasta que no haya bloqueos.".to_string(),
            ],
            no_push: true,
            summary: format!("{} bloqueado antes de gates o commit.", problem.id),
        });
    }

    let mut gate_results = Vec::new();
    for gate in &problem.gates {
        let result = run_gate(&inspection.repo_path, gate, None, None).await?;
        let passed = result.status == "passed";
        gate_results.push(result);
        if !passed {
            return Ok(LocalProblemRun {
                id: run_id(&inspection.repo_path, &problem.id),
                problem_id: problem.id.clone(),
                status: "blocked".to_string(),
                repo_path: inspection.repo_path,
                branch: inspection.current_branch,
                commit_sha: None,
                changed_files,
                gate_results,
                blockers: vec![format!("Gate {gate} no paso; commit local bloqueado.")],
                warnings: Vec::new(),
                next_actions: vec![
                    "Leer salida del gate, reducir el cambio y volver a correr commit LOCAL."
                        .to_string(),
                ],
                no_push: true,
                summary: format!("{} bloqueado por gate.", problem.id),
            });
        }
    }

    git_add_files(&repo, &changed_files)?;
    git_commit(&repo, &problem.commit_message)?;
    let commit_sha = git(&repo, &["rev-parse", "HEAD"])?;

    Ok(LocalProblemRun {
        id: run_id(&inspection.repo_path, &problem.id),
        problem_id: problem.id.clone(),
        status: "committed".to_string(),
        repo_path: inspection.repo_path,
        branch: inspection.current_branch,
        commit_sha: Some(commit_sha),
        changed_files,
        gate_results,
        blockers: Vec::new(),
        warnings: vec!["Commit local creado; no se hizo push automatico.".to_string()],
        next_actions: vec![
            "Revisar diff y commit local.".to_string(),
            "Crear PR manual solo si el resultado clinico-tecnico es correcto.".to_string(),
        ],
        no_push: true,
        summary: format!("{} resuelto con commit local.", problem.id),
    })
}

fn local_problem_specs() -> Vec<LocalProblemSpec> {
    vec![
        spec(
            "LOCAL-001",
            "dieta clinical_intent.py fase 3",
            "Extraer builders o helpers deterministicos restantes sin cambiar API ni prompts.",
            "agent/local-001-dieta-clinical-intent-py-fase-3",
            "LOCAL-001 diet clinical_intent helpers",
            &["clinical_intent.py"],
            &["clinical_intent"],
            &["check:api", "check:contract"],
            &["prompt", "endpoint nuevo", "openapi"],
        ),
        spec(
            "LOCAL-002",
            "adelgazar clinical_record.py",
            "Mover enums/tipos auxiliares a modulo de dominio pequeno sin cambiar SQLAlchemy ni migraciones.",
            "agent/local-002-adelgazar-clinical-record-py",
            "LOCAL-002 slim clinical_record domain types",
            &["clinical_record.py"],
            &["clinical_record"],
            &["check:api"],
            &["sqlalchemy", "migration", "alembic", "table", "column"],
        ),
        spec(
            "LOCAL-003",
            "dividir clinical-intent-result-panel.tsx",
            "Extraer subpaneles visuales pequenos: faltantes, decisiones, evidencia. Sin cambiar textos clinicos.",
            "agent/local-003-dividir-clinical-intent-result-panel",
            "LOCAL-003 split clinical intent result panel",
            &["clinical-intent-result-panel.tsx"],
            &["clinical-intent-result"],
            &["check:web"],
            &["texto clinico", "clinical copy", "endpoint"],
        ),
        spec(
            "LOCAL-004",
            "adelgazar patient-ai-chart-pages.tsx",
            "Mantenerlo como orquestador; extraer cabecera/estado operativo si aplica.",
            "agent/local-004-adelgazar-patient-ai-chart-pages",
            "LOCAL-004 slim patient ai chart pages",
            &["patient-ai-chart-pages.tsx"],
            &["patient-ai-chart"],
            &["check:web", "check:size"],
            &["endpoint", "dashboard", "rag"],
        ),
        spec(
            "LOCAL-005",
            "dividir assistant-read-sections.tsx",
            "Separar timeline/search/series/correlation en archivos de dominio, sin crear carpeta generica utils.",
            "agent/local-005-dividir-assistant-read-sections",
            "LOCAL-005 split assistant read sections",
            &["assistant-read-sections.tsx"],
            &["assistant-read", "timeline", "correlation", "series"],
            &["check:web", "check:e2e"],
            &["utils", "rag", "endpoint"],
        ),
        spec(
            "LOCAL-006",
            "adelgazar patient-record-workspaces.tsx",
            "Extraer solo un workspace claro, por ejemplo auditoria o sugerencias IA. Refactor puro.",
            "agent/local-006-adelgazar-patient-record-workspaces",
            "LOCAL-006 slim patient record workspaces",
            &["patient-record-workspaces.tsx"],
            &["patient-record-workspace"],
            &["check:web"],
            &["endpoint", "permission", "route"],
        ),
        spec(
            "LOCAL-007",
            "dieta ambulatory-appointment-pages.tsx",
            "Separar lista y formulario de cita sin tocar permisos ni endpoints.",
            "agent/local-007-dieta-ambulatory-appointment-pages",
            "LOCAL-007 split ambulatory appointment pages",
            &["ambulatory-appointment-pages.tsx"],
            &["ambulatory-appointment"],
            &["check:web"],
            &["endpoint", "permission", "permiso"],
        ),
        spec(
            "LOCAL-008",
            "dieta ambulatory-visit-pages.tsx",
            "Extraer panel de preconsulta o cierre ambulatorio, sin cambiar enfermeria/permisos.",
            "agent/local-008-dieta-ambulatory-visit-pages",
            "LOCAL-008 split ambulatory visit pages",
            &["ambulatory-visit-pages.tsx"],
            &["ambulatory-visit"],
            &["check:web", "check:e2e"],
            &["endpoint", "permission", "enfermeria"],
        ),
        spec(
            "LOCAL-009",
            "revisar demo-record.ts",
            "Verificar que no haya contaminacion de nombres/proyectos externos y dividir datos demo si el archivo sigue creciendo.",
            "agent/local-009-revisar-demo-record",
            "LOCAL-009 review demo record",
            &["demo-record.ts"],
            &["demo-record"],
            &["check:web"],
            &["real patient", "phi", "externo"],
        ),
        spec(
            "LOCAL-010",
            "robustecer smoke de ficha",
            "Mejorar selectores Playwright ambiguos en ficha/papel sin agregar cobertura pesada.",
            "agent/local-010-robustecer-smoke-ficha",
            "LOCAL-010 harden patient chart smoke",
            &["*.spec.ts", "*.e2e.ts", "playwright"],
            &["smoke", "ficha", "paper", "patient", "playwright"],
            &["check:e2e"],
            &["coverage pesada", "new feature", "dashboard"],
        ),
    ]
}

fn spec(
    id: &str,
    title: &str,
    objective: &str,
    branch: &str,
    commit_message: &str,
    primary_files: &[&str],
    allowed_path_markers: &[&str],
    gates: &[&str],
    forbidden_signals: &[&str],
) -> LocalProblemSpec {
    LocalProblemSpec {
        id: id.to_string(),
        title: title.to_string(),
        objective: objective.to_string(),
        branch: branch.to_string(),
        commit_message: commit_message.to_string(),
        primary_files: primary_files.iter().map(|item| item.to_string()).collect(),
        allowed_path_markers: allowed_path_markers
            .iter()
            .map(|item| item.to_string())
            .collect(),
        gates: gates.iter().map(|item| item.to_string()).collect(),
        forbidden_signals: GLOBAL_FORBIDDEN_SIGNALS
            .iter()
            .chain(forbidden_signals.iter())
            .map(|item| item.to_string())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        instructions: vec![
            "Prioridad: dieta y claridad antes de clinica nueva.".to_string(),
            "No crear endpoint, tabla, ruta, permisos, IA nueva, RAG, receta, firma ni dashboard."
                .to_string(),
            "Cada problema se resuelve en una rama agent/local-* y un commit local.".to_string(),
            "No hacer push automatico.".to_string(),
        ],
    }
}

fn local_problem_spec(problem_id: &str) -> Result<LocalProblemSpec, String> {
    local_problem_specs()
        .into_iter()
        .find(|problem| problem.id.eq_ignore_ascii_case(problem_id))
        .ok_or_else(|| format!("Problema LOCAL no registrado: {problem_id}."))
}

fn common_blocks(
    inspection: &crate::agent::types::RepoInspection,
    problem: &LocalProblemSpec,
) -> Vec<String> {
    let mut blockers = Vec::new();
    if !inspection.is_git_repo {
        blockers.push("El proyecto objetivo debe ser Git.".to_string());
    }
    if !inspection.is_one_epis {
        blockers.push("LOCAL-* solo se ejecuta con adaptador OneEpis activo.".to_string());
    }
    for gate in &problem.gates {
        if !inspection.declared_gates.contains(gate) {
            blockers.push(format!("Gate requerido no declarado: {gate}."));
        }
    }
    blockers.extend(
        inspection
            .blocks
            .iter()
            .filter(|block| !block.to_ascii_lowercase().contains("worktree sucio"))
            .cloned(),
    );
    blockers
}

fn blocked_run(
    repo_path: &str,
    problem: &LocalProblemSpec,
    blockers: Vec<String>,
) -> LocalProblemRun {
    LocalProblemRun {
        id: run_id(repo_path, &problem.id),
        problem_id: problem.id.clone(),
        status: "blocked".to_string(),
        repo_path: repo_path.to_string(),
        branch: problem.branch.clone(),
        commit_sha: None,
        changed_files: Vec::new(),
        gate_results: Vec::new(),
        blockers,
        warnings: Vec::new(),
        next_actions: vec![
            "Resolver bloqueos antes de preparar la rama local.".to_string(),
            "No aplicar ni commitear cambios fuera de LOCAL-*.".to_string(),
        ],
        no_push: true,
        summary: format!("{} bloqueado.", problem.id),
    }
}

fn ensure_problem_branch(
    repo: &Path,
    current_branch: &str,
    target_branch: &str,
) -> Result<String, String> {
    if current_branch == target_branch {
        return Ok(target_branch.to_string());
    }
    if git(repo, &["rev-parse", "--verify", target_branch]).is_ok() {
        git(repo, &["switch", target_branch])?;
    } else {
        git(repo, &["switch", "-c", target_branch])?;
    }
    Ok(target_branch.to_string())
}

fn changed_files(repo: &Path) -> Result<Vec<String>, String> {
    let status = git(repo, &["status", "--porcelain", "--untracked-files=all"])?;
    let mut files = Vec::new();
    for line in status.lines() {
        if line.len() < 4 {
            continue;
        }
        let raw = line[3..].trim();
        let path = raw
            .split(" -> ")
            .last()
            .unwrap_or(raw)
            .trim_matches('"')
            .replace('\\', "/");
        if !path.is_empty() {
            files.push(path);
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn validate_changed_files(problem: &LocalProblemSpec, files: &[String]) -> Vec<String> {
    let mut blockers = Vec::new();
    for file in files {
        let lower = file.to_ascii_lowercase();
        if problem
            .forbidden_signals
            .iter()
            .any(|signal| lower.contains(signal))
        {
            blockers.push(format!("Archivo fuera de gobernanza LOCAL: {file}."));
            continue;
        }
        if !problem
            .allowed_path_markers
            .iter()
            .any(|marker| lower.contains(&marker.to_ascii_lowercase()))
        {
            blockers.push(format!("Archivo no permitido para {}: {file}.", problem.id));
        }
    }
    blockers
}

fn validate_diff_content(
    repo: &Path,
    problem: &LocalProblemSpec,
    files: &[String],
) -> Result<Vec<String>, String> {
    let mut content = diff_signal_lines(&git(repo, &["diff", "--unified=0", "--"])?);
    content.push_str(&diff_signal_lines(&git(
        repo,
        &["diff", "--cached", "--unified=0", "--"],
    )?));
    for file in files {
        let path = repo.join(file);
        if path.is_file() {
            if let Ok(text) = fs::read_to_string(path) {
                content.push('\n');
                content.push_str(&text);
            }
        }
    }
    let lower = content.to_ascii_lowercase();
    Ok(problem
        .forbidden_signals
        .iter()
        .filter(|signal| lower.contains(&signal.to_ascii_lowercase()))
        .map(|signal| format!("Senal prohibida para {}: {signal}.", problem.id))
        .collect())
}

fn diff_signal_lines(diff: &str) -> String {
    diff.lines()
        .filter(|line| {
            (line.starts_with('+') && !line.starts_with("+++"))
                || (line.starts_with('-') && !line.starts_with("---"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn git_add_files(repo: &Path, files: &[String]) -> Result<(), String> {
    let mut args = vec!["add", "--"];
    args.extend(files.iter().map(String::as_str));
    git(repo, &args)?;
    Ok(())
}

fn git_commit(repo: &Path, message: &str) -> Result<(), String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("-c")
        .arg("user.name=OneEpis Local Agent")
        .arg("-c")
        .arg("user.email=oneepis-local-agent@example.invalid")
        .arg("commit")
        .arg("-m")
        .arg(message)
        .output()
        .map_err(|err| format!("No se pudo ejecutar git commit: {err}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = sanitize_log(&String::from_utf8_lossy(&output.stderr));
    Err(format!("git commit fallo: {stderr}"))
}

fn run_id(repo_path: &str, problem_id: &str) -> String {
    let digest = sha256_hex(format!("{repo_path}:{problem_id}:{}", Utc::now()).as_bytes());
    format!("local-{}", &digest[..16])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_contains_all_requested_local_problems() {
        let specs = list_local_problems();
        let ids = specs
            .iter()
            .map(|spec| spec.id.as_str())
            .collect::<BTreeSet<_>>();

        for index in 1..=10 {
            assert!(ids.contains(format!("LOCAL-{index:03}").as_str()));
        }
        assert!(specs
            .iter()
            .all(|spec| spec.branch.starts_with("agent/local-")));
        assert!(specs.iter().all(|spec| !spec.gates.is_empty()));
        assert!(specs.iter().all(|spec| spec
            .instructions
            .iter()
            .any(|item| item.contains("No hacer push"))));
    }

    #[tokio::test]
    async fn prepare_creates_safe_branch_without_commit() {
        let repo = temp_oneepis_repo("prepare");
        let before = git(&repo, &["rev-parse", "HEAD"]).expect("head");
        let result = prepare_local_problem(LocalProblemRequest {
            repo_path: repo.display().to_string(),
            problem_id: "LOCAL-003".to_string(),
        })
        .await
        .expect("prepare");

        assert_eq!(result.status, "ready_for_changes");
        assert_eq!(
            result.branch,
            "agent/local-003-dividir-clinical-intent-result-panel"
        );
        assert!(result.no_push);
        assert_eq!(git(&repo, &["rev-parse", "HEAD"]).expect("head"), before);
        let _ = fs::remove_dir_all(repo);
    }

    #[tokio::test]
    async fn commit_blocks_changes_outside_problem_surface() {
        let repo = temp_oneepis_repo("forbidden");
        prepare_local_problem(LocalProblemRequest {
            repo_path: repo.display().to_string(),
            problem_id: "LOCAL-003".to_string(),
        })
        .await
        .expect("prepare");
        fs::create_dir_all(repo.join("migrations")).expect("migrations");
        fs::write(repo.join("migrations").join("001.sql"), "select 1;").expect("migration");

        let result = commit_local_problem(LocalProblemRequest {
            repo_path: repo.display().to_string(),
            problem_id: "LOCAL-003".to_string(),
        })
        .await
        .expect("commit");

        assert_eq!(result.status, "blocked");
        assert!(result.commit_sha.is_none());
        assert!(result
            .blockers
            .iter()
            .any(|block| block.contains("Archivo fuera de gobernanza LOCAL")));
        let _ = fs::remove_dir_all(repo);
    }

    #[tokio::test]
    async fn commit_runs_gates_and_creates_local_commit_without_push() {
        let repo = temp_oneepis_repo("commit");
        prepare_local_problem(LocalProblemRequest {
            repo_path: repo.display().to_string(),
            problem_id: "LOCAL-003".to_string(),
        })
        .await
        .expect("prepare");
        fs::create_dir_all(repo.join("web")).expect("web");
        fs::write(
            repo.join("web")
                .join("clinical-intent-result-panel-parts.tsx"),
            "export const MissingEvidencePanel = () => null;\n",
        )
        .expect("component");

        let result = commit_local_problem(LocalProblemRequest {
            repo_path: repo.display().to_string(),
            problem_id: "LOCAL-003".to_string(),
        })
        .await
        .expect("commit");

        assert_eq!(result.status, "committed");
        assert!(result.no_push);
        assert!(result.commit_sha.is_some());
        assert!(result
            .gate_results
            .iter()
            .any(|gate| gate.gate == "check:web" && gate.status == "passed"));
        assert!(git(&repo, &["status", "--short"])
            .expect("status")
            .is_empty());
        let _ = fs::remove_dir_all(repo);
    }

    fn temp_oneepis_repo(label: &str) -> std::path::PathBuf {
        let repo = std::env::temp_dir().join(format!(
            "oneepis-agent-local-problem-{label}-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(repo.join("docs")).expect("docs");
        Command::new("git")
            .arg("init")
            .current_dir(&repo)
            .output()
            .expect("git init");
        fs::write(repo.join("AGENTS.md"), "# Agents\n").expect("agents");
        fs::write(repo.join("docs").join("GOVERNANCE.md"), "# Governance\n").expect("gov");
        fs::write(
            repo.join("package.json"),
            r#"{"scripts":{"check:api":"echo api","check:contract":"echo contract","check:web":"echo web","check:e2e":"echo e2e","check:size":"echo size"}}"#,
        )
        .expect("package");
        commit_all(&repo);
        repo
    }

    fn commit_all(repo: &Path) {
        git(repo, &["add", "."]).expect("git add");
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .arg("-c")
            .arg("user.name=OneEpis Agent Test")
            .arg("-c")
            .arg("user.email=oneepis-agent-test@example.invalid")
            .arg("commit")
            .arg("-m")
            .arg("fixture")
            .output()
            .expect("git commit");
        assert!(output.status.success(), "git commit failed");
    }
}
