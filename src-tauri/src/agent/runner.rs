use crate::agent::ollama::{ask_for_micro_plan, ask_for_patch_plan};
use crate::agent::persistence::record_run;
use crate::agent::repo::{ensure_oneepis_checkout, git, inspect_repository};
use crate::agent::safety::{sanitize_log, sha256_hex};
use crate::agent::types::{
    AgentRun, AgentStep, AutopilotRequest, DevelopmentTask, LocalCommitResult, MicroPlan,
    NextWork, PatchPlan, RunRequest,
};
use chrono::Utc;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

const STATES: &[&str] = &[
    "preflight",
    "governance_read",
    "repo_audit",
    "micro_plan",
    "patch_draft",
    "safety_review",
    "apply_patch",
    "gate_run",
    "result_record",
    "lesson_record",
    "stop_or_next",
];

pub async fn plan_microcycle(
    repo_path: &str,
    objective: &str,
    base_url: Option<String>,
) -> Result<MicroPlan, String> {
    let inspection = inspect_repository(repo_path)?;
    if let Some(mut plan) = ask_for_micro_plan(base_url, &inspection, objective).await {
        normalize_plan(&mut plan, &inspection.declared_gates);
        if !inspection.blocks.is_empty() {
            plan.blocked = true;
            plan.warnings.extend(inspection.blocks.clone());
        }
        return Ok(plan);
    }
    Ok(fallback_plan(&inspection, objective))
}

pub async fn run_microcycle(request: RunRequest) -> Result<AgentRun, String> {
    let max_cycles = request.max_cycles.unwrap_or(1).clamp(1, 3);
    let mode = request.mode.unwrap_or_else(|| "dry_run".to_string());
    let started_at = Utc::now().to_rfc3339();
    let inspection = inspect_repository(&request.repo_path)?;
    let plan = plan_microcycle(&request.repo_path, &request.objective, None).await?;
    let blocked = plan.blocked || !inspection.blocks.is_empty() || mode != "dry_run";
    let mut steps = Vec::new();

    for (index, state) in STATES.iter().enumerate() {
        let status = state_status(state, blocked, &mode);
        steps.push(AgentStep {
            order: index + 1,
            state: state.to_string(),
            status: status.to_string(),
            summary: state_summary(state, &inspection, &plan, max_cycles, &mode),
        });
        if blocked && *state == "safety_review" {
            break;
        }
    }

    let mut lessons = vec![
        "Registrar primero el contexto y la gobernanza reduce cambios innecesarios.".to_string(),
        "La primera version ejecuta dry-run: los patches reales quedan bloqueados hasta v0.3.".to_string(),
    ];
    if inspection.is_one_epis {
        lessons.push("OneEpis requiere microciclos pequenos y gates oficiales antes de crecer.".to_string());
    }
    if max_cycles > 1 {
        lessons.push(format!(
            "Se pidieron {max_cycles} ciclos, pero v0.1 registra solo una pasada segura."
        ));
    }

    let completed_at = Utc::now().to_rfc3339();
    let id = run_id(&inspection.repo_path, &request.objective, &started_at);
    let mut run = AgentRun {
        id,
        repo_path: inspection.repo_path.clone(),
        objective: sanitize_log(&request.objective),
        branch: inspection.current_branch.clone(),
        status: if blocked { "blocked" } else { "completed" }.to_string(),
        mode,
        model_used: plan.model_used.clone(),
        started_at,
        completed_at,
        steps,
        plan,
        checkout: None,
        next_work: None,
        task: None,
        patch_plan: None,
        commit_result: None,
        changed_files: Vec::new(),
        lessons,
        persistence: "pending".to_string(),
    };

    run.persistence = match record_run(request.database_url, &run).await {
        Ok(status) => status,
        Err(err) => format!("not_recorded: {}", sanitize_log(&err)),
    };
    Ok(run)
}

pub async fn run_oneepis_autopilot(request: AutopilotRequest) -> Result<AgentRun, String> {
    let max_cycles = request.max_cycles.unwrap_or(1).clamp(1, 3);
    let mode = request.mode.unwrap_or_else(|| "controlled".to_string());
    let objective = request
        .objective
        .unwrap_or_else(|| "Clonar, auditar y ejecutar el siguiente trabajo local verificable de OneEpis.".to_string());
    let started_at = Utc::now().to_rfc3339();
    let checkout = ensure_oneepis_checkout(
        request.workspace_path.as_deref(),
        request.repo_url.as_deref(),
    )?;
    let inspection = inspect_repository(&checkout.repo_path)?;
    let plan = plan_microcycle(&checkout.repo_path, &objective, None).await?;
    let next_work = choose_next_work(&inspection, &plan);
    let mut steps = Vec::new();
    let mut lessons = Vec::new();
    let mut status = "completed".to_string();

    push_step(&mut steps, "preflight", "completed", &format!("Repo OneEpis en {}.", checkout.repo_path));
    push_step(&mut steps, "checkout", "completed", &format!("Checkout {}.", checkout.action));
    push_step(
        &mut steps,
        "governance_read",
        "completed",
        &format!(
            "{} documentos de gobernanza presentes.",
            inspection.governance_documents.iter().filter(|doc| doc.present).count()
        ),
    );
    push_step(&mut steps, "repo_audit", "completed", &format!("Rama {}.", inspection.current_branch));
    push_step(&mut steps, "micro_plan", "completed", &format!("Plan generado con {}.", plan.model_used));

    if !inspection.blocks.is_empty() || plan.blocked {
        status = "blocked".to_string();
        push_step(
            &mut steps,
            "safety_review",
            "blocked",
            "El repo o el plan tiene bloqueos; no se ejecutan gates.",
        );
    } else if mode != "controlled" {
        status = "blocked".to_string();
        push_step(
            &mut steps,
            "safety_review",
            "blocked",
            "Autopilot solo ejecuta en modo controlled.",
        );
    } else {
        push_step(
            &mut steps,
            "safety_review",
            "completed",
            &format!("Siguiente trabajo: {}.", next_work.title),
        );
        let gate_result = run_gate(&checkout.repo_path, &next_work);
        match gate_result {
            Ok(summary) => {
                push_step(&mut steps, "gate_run", "completed", &summary);
                lessons.push(format!(
                    "Trabajo ejecutado localmente: {} via {}.",
                    next_work.title,
                    next_work.command.join(" ")
                ));
            }
            Err(err) => {
                status = "failed".to_string();
                push_step(&mut steps, "gate_run", "failed", &err);
                lessons.push("El siguiente ciclo debe reparar el gate fallido antes de ampliar alcance.".to_string());
            }
        }
    }

    push_step(&mut steps, "result_record", "completed", "Resultado preparado para bitacora.");
    push_step(&mut steps, "stop_or_next", "completed", &format!("Parada tras {max_cycles} ciclo(s) solicitado(s)."));
    if inspection.is_one_epis {
        lessons.push("OneEpis se trabaja con un gate local verificable antes de cualquier PR.".to_string());
    }

    let completed_at = Utc::now().to_rfc3339();
    let id = run_id(&checkout.repo_path, &objective, &started_at);
    let mut run = AgentRun {
        id,
        repo_path: checkout.repo_path.clone(),
        objective: sanitize_log(&objective),
        branch: inspection.current_branch,
        status,
        mode,
        model_used: plan.model_used.clone(),
        started_at,
        completed_at,
        steps,
        plan,
        checkout: Some(checkout),
        next_work: Some(next_work),
        task: None,
        patch_plan: None,
        commit_result: None,
        changed_files: Vec::new(),
        lessons,
        persistence: "pending".to_string(),
    };

    run.persistence = match record_run(request.database_url, &run).await {
        Ok(status) => status,
        Err(err) => format!("not_recorded: {}", sanitize_log(&err)),
    };
    Ok(run)
}

pub async fn run_oneepis_dev_autopilot(request: AutopilotRequest) -> Result<AgentRun, String> {
    let max_cycles = request.max_cycles.unwrap_or(1).clamp(1, 3);
    let mode = request.mode.unwrap_or_else(|| "local_commit".to_string());
    let objective = request.objective.unwrap_or_else(|| {
        "Clonar, auditar, aplicar un microcambio con Ollama local, validar y dejar commit local."
            .to_string()
    });
    let started_at = Utc::now().to_rfc3339();
    let checkout = ensure_oneepis_checkout(
        request.workspace_path.as_deref(),
        request.repo_url.as_deref(),
    )?;
    let inspection = inspect_repository(&checkout.repo_path)?;
    let plan = plan_microcycle(&checkout.repo_path, &objective, None).await?;
    let task = choose_development_task(&inspection, &plan);
    let branch_name = development_branch_name(&task, &started_at);
    let mut steps = Vec::new();
    let mut lessons = Vec::new();
    let mut status = "completed".to_string();
    let mut patch_plan = None;
    let mut commit_result = None;
    let mut changed_files = Vec::new();

    push_step(&mut steps, "preflight", "completed", &format!("Repo OneEpis en {}.", checkout.repo_path));
    push_step(&mut steps, "checkout", "completed", &format!("Checkout {}.", checkout.action));
    push_step(&mut steps, "task_select", "completed", &format!("{}: {}.", task.id, task.title));

    if !inspection.blocks.is_empty() || plan.blocked {
        status = "blocked".to_string();
        push_step(
            &mut steps,
            "safety_review",
            "blocked",
            "El repo o el plan tiene bloqueos; no se crean ramas ni patches.",
        );
    } else if mode != "local_commit" {
        status = "blocked".to_string();
        push_step(
            &mut steps,
            "safety_review",
            "blocked",
            "autopilot-dev solo ejecuta en modo local_commit.",
        );
    } else {
        push_step(&mut steps, "safety_review", "completed", "Worktree limpio y tarea verde.");
        let Some(candidate_plan) =
            ask_for_patch_plan(None, &inspection, &task, &branch_name).await
        else {
            status = "blocked".to_string();
            push_step(
                &mut steps,
                "patch_plan",
                "blocked",
                "Ollama local no entrego un PatchPlan valido; no se aplican cambios.",
            );
            return finalize_dev_run(
                request.database_url,
                DevRunParts {
                    checkout,
                    inspection,
                    objective,
                    mode,
                    status,
                    started_at,
                    steps,
                    plan,
                    task,
                    patch_plan,
                    commit_result,
                    changed_files,
                    lessons,
                },
            )
            .await;
        };

        match validate_patch_plan(&candidate_plan, &task, &branch_name) {
            Ok(()) => {
                push_step(&mut steps, "patch_plan", "completed", &candidate_plan.summary);
            }
            Err(err) => {
                status = "blocked".to_string();
                push_step(&mut steps, "patch_plan", "blocked", &err);
                patch_plan = Some(candidate_plan);
                return finalize_dev_run(
                    request.database_url,
                    DevRunParts {
                        checkout,
                        inspection,
                        objective,
                        mode,
                        status,
                        started_at,
                        steps,
                        plan,
                        task,
                        patch_plan,
                        commit_result,
                        changed_files,
                        lessons,
                    },
                )
                .await;
            }
        }

        let repo_path = Path::new(&checkout.repo_path);
        git(repo_path, &["checkout", "-b", &branch_name])?;
        push_step(&mut steps, "branch_create", "completed", &format!("Rama local {branch_name}."));
        apply_patch_plan(repo_path, &candidate_plan)?;
        changed_files = changed_files(repo_path)?;
        push_step(
            &mut steps,
            "apply_patch",
            "completed",
            &format!("Archivos cambiados: {}.", changed_files.join(", ")),
        );
        patch_plan = Some(candidate_plan.clone());

        let next_work = NextWork {
            kind: "development_gate".to_string(),
            title: format!("Validar {}", task.required_gate),
            rationale: "Gate requerido por la tarea local antes de commit.".to_string(),
            gate: task.required_gate.clone(),
            command: vec!["npm".to_string(), "run".to_string(), task.required_gate.clone()],
        };
        match run_gate(&checkout.repo_path, &next_work) {
            Ok(gate_summary) => {
                push_step(&mut steps, "gate_run", "completed", &gate_summary);
                if let Err(err) = git_add_changed(repo_path, &changed_files) {
                    status = "failed".to_string();
                    push_step(&mut steps, "commit", "failed", &err);
                    lessons.push("No se creo commit porque no se pudieron preparar los archivos.".to_string());
                    patch_plan = Some(candidate_plan);
                    return finalize_dev_run(
                        request.database_url,
                        DevRunParts {
                            checkout,
                            inspection,
                            objective,
                            mode,
                            status,
                            started_at,
                            steps,
                            plan,
                            task,
                            patch_plan,
                            commit_result,
                            changed_files,
                            lessons,
                        },
                    )
                    .await;
                }
                let message = format!("{}\n\nGate: npm run {}", task.title, task.required_gate);
                match git(repo_path, &["commit", "-m", &message]) {
                    Ok(_) => {
                        let commit_sha = git(repo_path, &["rev-parse", "--short", "HEAD"])?;
                        commit_result = Some(LocalCommitResult {
                            branch: branch_name.clone(),
                            commit_sha: commit_sha.trim().to_string(),
                            status: "committed".to_string(),
                            gate_command: next_work.command.clone(),
                            gate_output_summary: gate_summary,
                        });
                        push_step(&mut steps, "commit", "completed", &format!("Commit local en {branch_name}."));
                        lessons.push("El ciclo dejo un commit local; el humano decide si publica PR remoto.".to_string());
                    }
                    Err(err) => {
                        status = "failed".to_string();
                        push_step(&mut steps, "commit", "failed", &err);
                        lessons.push("No se creo commit; revisa configuracion local de Git o el estado del index.".to_string());
                    }
                }
            }
            Err(err) => {
                status = "failed".to_string();
                push_step(&mut steps, "gate_run", "failed", &err);
                lessons.push("No se creo commit porque el gate requerido fallo.".to_string());
            }
        }
    }

    push_step(&mut steps, "result_record", "completed", "Resultado preparado para bitacora.");
    push_step(&mut steps, "stop_or_next", "completed", &format!("Parada tras {max_cycles} ciclo(s) solicitado(s)."));

    finalize_dev_run(
        request.database_url,
        DevRunParts {
            checkout,
            inspection,
            objective,
            mode,
            status,
            started_at,
            steps,
            plan,
            task,
            patch_plan,
            commit_result,
            changed_files,
            lessons,
        },
    )
    .await
}

fn fallback_plan(inspection: &crate::agent::types::RepoInspection, objective: &str) -> MicroPlan {
    let recommended_gate = select_gate(&inspection.declared_gates);
    let mut warnings = inspection.blocks.clone();
    if !inspection.is_one_epis {
        warnings.push("Repo generico: se aplican reglas basicas de safety, no doctrina OneEpis completa.".to_string());
    }
    MicroPlan {
        objective: sanitize_log(objective),
        recommended_gate,
        steps: vec![
            "Leer documentos de gobernanza detectados.".to_string(),
            "Confirmar estado Git y rama de trabajo.".to_string(),
            "Elegir un unico cambio pequeno y reversible.".to_string(),
            "Correr el gate minimo declarado por el repo.".to_string(),
            "Registrar resultado y aprendizaje antes de continuar.".to_string(),
        ],
        warnings,
        blocked: !inspection.blocks.is_empty(),
        model_used: "local_rules".to_string(),
    }
}

struct DevRunParts {
    checkout: crate::agent::types::RepoCheckout,
    inspection: crate::agent::types::RepoInspection,
    objective: String,
    mode: String,
    status: String,
    started_at: String,
    steps: Vec<AgentStep>,
    plan: MicroPlan,
    task: DevelopmentTask,
    patch_plan: Option<PatchPlan>,
    commit_result: Option<LocalCommitResult>,
    changed_files: Vec<String>,
    lessons: Vec<String>,
}

async fn finalize_dev_run(
    database_url: Option<String>,
    parts: DevRunParts,
) -> Result<AgentRun, String> {
    let completed_at = Utc::now().to_rfc3339();
    let id = run_id(&parts.checkout.repo_path, &parts.objective, &parts.started_at);
    let branch = parts
        .commit_result
        .as_ref()
        .map(|result| result.branch.clone())
        .unwrap_or_else(|| parts.inspection.current_branch.clone());
    let mut run = AgentRun {
        id,
        repo_path: parts.checkout.repo_path.clone(),
        objective: sanitize_log(&parts.objective),
        branch,
        status: parts.status,
        mode: parts.mode,
        model_used: parts
            .patch_plan
            .as_ref()
            .map(|patch| patch.model_used.clone())
            .filter(|model| !model.is_empty())
            .unwrap_or_else(|| parts.plan.model_used.clone()),
        started_at: parts.started_at,
        completed_at,
        steps: parts.steps,
        plan: parts.plan,
        checkout: Some(parts.checkout),
        next_work: None,
        task: Some(parts.task),
        patch_plan: parts.patch_plan,
        commit_result: parts.commit_result,
        changed_files: parts.changed_files,
        lessons: parts.lessons,
        persistence: "pending".to_string(),
    };

    run.persistence = match record_run(database_url, &run).await {
        Ok(status) => status,
        Err(err) => format!("not_recorded: {}", sanitize_log(&err)),
    };
    Ok(run)
}

fn choose_development_task(
    inspection: &crate::agent::types::RepoInspection,
    plan: &MicroPlan,
) -> DevelopmentTask {
    let required_gate = select_gate(&inspection.declared_gates);
    let mut files = vec![
        "docs/CODEX_PLAN.md".to_string(),
        "docs/CURRENT_STATE.md".to_string(),
        "AGENTS.md".to_string(),
    ];
    files.retain(|path| Path::new(&inspection.repo_path).join(path).exists());
    if files.is_empty() {
        files.push("README.md".to_string());
    }

    DevelopmentTask {
        id: "oneepis-local-next-step".to_string(),
        title: "Documentar siguiente microtrabajo local de OneEpis".to_string(),
        surface: "governance".to_string(),
        risk: "verde".to_string(),
        rationale: format!(
            "Trabajo local pequeno derivado de la auditoria: {}",
            if plan.objective.is_empty() {
                "mantener el ciclo gobernado sin abrir superficies nuevas"
            } else {
                plan.objective.as_str()
            }
        ),
        files,
        required_gate,
        allowed_actions: vec!["text_replace".to_string()],
    }
}

fn development_branch_name(task: &DevelopmentTask, started_at: &str) -> String {
    let digest = sha256_hex(format!("{}:{started_at}", task.id).as_bytes());
    format!("agent/{}-{}", task.id, &digest[..8])
}

fn validate_patch_plan(
    patch_plan: &PatchPlan,
    task: &DevelopmentTask,
    expected_branch: &str,
) -> Result<(), String> {
    if patch_plan.task_id != task.id {
        return Err("PatchPlan rechazado: taskId no coincide con la tarea seleccionada.".to_string());
    }
    if patch_plan.branch_name != expected_branch {
        return Err("PatchPlan rechazado: branchName no coincide con la rama segura esperada.".to_string());
    }
    if patch_plan.expected_gate != task.required_gate {
        return Err("PatchPlan rechazado: expectedGate no coincide con el gate requerido.".to_string());
    }
    if patch_plan.edits.is_empty() {
        return Err("PatchPlan rechazado: no contiene edits.".to_string());
    }
    if patch_plan.edits.len() > 3 {
        return Err("PatchPlan rechazado: supera el limite de 3 edits.".to_string());
    }
    if !patch_plan.forbidden_edits.is_empty() {
        return Err("PatchPlan rechazado: declara forbiddenEdits no vacios.".to_string());
    }

    for edit in &patch_plan.edits {
        validate_relative_path(&edit.path)?;
        if !task.files.iter().any(|allowed| allowed == &edit.path) {
            return Err(format!("PatchPlan rechazado: archivo no permitido {}.", edit.path));
        }
        if edit.original.trim().is_empty() || edit.replacement.trim().is_empty() {
            return Err("PatchPlan rechazado: original/replacement no pueden estar vacios.".to_string());
        }
        if edit.original == edit.replacement {
            return Err("PatchPlan rechazado: edit sin cambio real.".to_string());
        }
    }

    Ok(())
}

fn validate_relative_path(path: &str) -> Result<(), String> {
    let path_buf = PathBuf::from(path);
    if path_buf.is_absolute()
        || path.contains("..")
        || path.contains('\\')
        || path.trim().is_empty()
    {
        return Err(format!("Ruta rechazada por safety: {path}"));
    }
    Ok(())
}

fn apply_patch_plan(repo_path: &Path, patch_plan: &PatchPlan) -> Result<(), String> {
    for edit in &patch_plan.edits {
        validate_relative_path(&edit.path)?;
        let file_path = repo_path.join(&edit.path);
        if !file_path.exists() {
            return Err(format!("No existe el archivo objetivo: {}", edit.path));
        }
        let text = fs::read_to_string(&file_path)
            .map_err(|err| format!("No se pudo leer {}: {err}", edit.path))?;
        let count = text.matches(&edit.original).count();
        if count != 1 {
            return Err(format!(
                "Edit rechazado en {}: original aparece {} veces.",
                edit.path, count
            ));
        }
        let updated = text.replacen(&edit.original, &edit.replacement, 1);
        fs::write(&file_path, updated)
            .map_err(|err| format!("No se pudo escribir {}: {err}", edit.path))?;
    }
    Ok(())
}

fn changed_files(repo_path: &Path) -> Result<Vec<String>, String> {
    let status = git(repo_path, &["status", "--short"])?;
    let files: Vec<String> = status
        .lines()
        .filter_map(|line| line.get(3..))
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(ToString::to_string)
        .collect();
    if files.is_empty() {
        return Err("No hay cambios despues de aplicar PatchPlan.".to_string());
    }
    Ok(files)
}

fn git_add_changed(repo_path: &Path, changed_files: &[String]) -> Result<(), String> {
    for path in changed_files {
        validate_relative_path(path)?;
        git(repo_path, &["add", path])?;
    }
    Ok(())
}

fn choose_next_work(inspection: &crate::agent::types::RepoInspection, plan: &MicroPlan) -> NextWork {
    let gate = if !plan.recommended_gate.is_empty() && plan.recommended_gate != "sin_gate" {
        plan.recommended_gate.clone()
    } else {
        select_gate(&inspection.declared_gates)
    };
    let command = if gate == "sin_gate" {
        Vec::new()
    } else if ["check", "test", "build"].contains(&gate.as_str()) || gate.starts_with("check:") {
        vec!["npm".to_string(), "run".to_string(), gate.clone()]
    } else {
        Vec::new()
    };

    NextWork {
        kind: "local_gate".to_string(),
        title: format!("Ejecutar gate local {gate}"),
        rationale: "Es el siguiente trabajo seguro porque valida el estado actual sin editar, hacer push ni depender de IA remota.".to_string(),
        gate,
        command,
    }
}

fn run_gate(repo_path: &str, next_work: &NextWork) -> Result<String, String> {
    if next_work.command.is_empty() {
        return Err("No hay gate ejecutable declarado por el repo.".to_string());
    }
    if next_work.command.first().map(String::as_str) != Some("npm")
        || next_work.command.get(1).map(String::as_str) != Some("run")
    {
        return Err("El trabajo seleccionado no corresponde a una accion tipada permitida.".to_string());
    }

    let executable = if cfg!(target_os = "windows") { "npm.cmd" } else { "npm" };
    let started = Instant::now();
    let output = Command::new(executable)
        .args(&next_work.command[1..])
        .current_dir(Path::new(repo_path))
        .output()
        .map_err(|err| format!("No se pudo ejecutar {}: {err}", next_work.command.join(" ")))?;
    let duration_ms = started.elapsed().as_millis();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let summary = sanitize_log(&format!(
        "{}\n{}\nDuracion: {duration_ms} ms",
        stdout.trim(),
        stderr.trim()
    ));
    if output.status.success() {
        Ok(format!("{} paso. {}", next_work.command.join(" "), summary))
    } else {
        Err(format!(
            "{} fallo con codigo {:?}. {}",
            next_work.command.join(" "),
            output.status.code(),
            summary
        ))
    }
}

fn push_step(steps: &mut Vec<AgentStep>, state: &str, status: &str, summary: &str) {
    steps.push(AgentStep {
        order: steps.len() + 1,
        state: state.to_string(),
        status: status.to_string(),
        summary: sanitize_log(summary),
    });
}

fn normalize_plan(plan: &mut MicroPlan, declared_gates: &[String]) {
    if plan.recommended_gate.is_empty() || !declared_gates.contains(&plan.recommended_gate) {
        plan.recommended_gate = select_gate(declared_gates);
    }
    if plan.steps.is_empty() {
        plan.steps = vec!["Reducir el objetivo a un microciclo verificable.".to_string()];
    }
}

fn select_gate(gates: &[String]) -> String {
    for preferred in ["check:size", "check:api", "check:web", "check", "test", "build"] {
        if gates.iter().any(|gate| gate == preferred) {
            return preferred.to_string();
        }
    }
    gates.first().cloned().unwrap_or_else(|| "sin_gate".to_string())
}

fn state_status(state: &str, blocked: bool, mode: &str) -> &'static str {
    if blocked && state == "safety_review" {
        return "blocked";
    }
    if blocked && matches!(state, "apply_patch" | "gate_run" | "result_record" | "lesson_record" | "stop_or_next") {
        return "skipped";
    }
    if mode != "dry_run" && matches!(state, "apply_patch" | "gate_run") {
        return "blocked";
    }
    "completed"
}

fn state_summary(
    state: &str,
    inspection: &crate::agent::types::RepoInspection,
    plan: &MicroPlan,
    max_cycles: u8,
    mode: &str,
) -> String {
    match state {
        "preflight" => format!("Repo {} en rama {}.", inspection.project_name, inspection.current_branch),
        "governance_read" => format!(
            "{} documentos de gobernanza presentes.",
            inspection.governance_documents.iter().filter(|doc| doc.present).count()
        ),
        "repo_audit" => {
            if inspection.dirty {
                "Worktree sucio; bloqueo preventivo activado.".to_string()
            } else {
                "Worktree limpio para planificar.".to_string()
            }
        }
        "micro_plan" => format!("Plan generado con modelo {}.", plan.model_used),
        "patch_draft" => "v0.1 no genera diff persistente; solo plan revisable.".to_string(),
        "safety_review" => {
            if mode != "dry_run" {
                "Modo controlado bloqueado hasta v0.3.".to_string()
            } else if plan.blocked {
                "Plan bloqueado por gobernanza o estado del repo.".to_string()
            } else {
                "Safety kernel permite dry-run sin escritura.".to_string()
            }
        }
        "apply_patch" => "No se aplica patch en dry-run.".to_string(),
        "gate_run" => format!("Gate recomendado para ejecucion futura: {}.", plan.recommended_gate),
        "result_record" => "Resultado preparado para bitacora.".to_string(),
        "lesson_record" => "Lecciones extraidas de una pasada acotada.".to_string(),
        "stop_or_next" => format!("Parada segura; presupuesto solicitado: {max_cycles} ciclo(s)."),
        _ => String::new(),
    }
}

fn run_id(repo_path: &str, objective: &str, started_at: &str) -> String {
    let digest = sha256_hex(format!("{repo_path}:{objective}:{started_at}").as_bytes());
    format!("run-{}", &digest[..16])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task() -> DevelopmentTask {
        DevelopmentTask {
            id: "oneepis-local-next-step".to_string(),
            title: "Documentar siguiente microtrabajo local de OneEpis".to_string(),
            surface: "governance".to_string(),
            risk: "verde".to_string(),
            rationale: "test".to_string(),
            files: vec!["docs/CODEX_PLAN.md".to_string()],
            required_gate: "check:size".to_string(),
            allowed_actions: vec!["text_replace".to_string()],
        }
    }

    fn patch_plan() -> PatchPlan {
        PatchPlan {
            task_id: "oneepis-local-next-step".to_string(),
            branch_name: "agent/oneepis-local-next-step-12345678".to_string(),
            summary: "test".to_string(),
            edits: vec![crate::agent::types::PatchEdit {
                path: "docs/CODEX_PLAN.md".to_string(),
                original: "old".to_string(),
                replacement: "new".to_string(),
            }],
            forbidden_edits: Vec::new(),
            expected_gate: "check:size".to_string(),
            model_used: "test".to_string(),
        }
    }

    #[test]
    fn validates_bounded_patch_plan() {
        assert!(validate_patch_plan(
            &patch_plan(),
            &task(),
            "agent/oneepis-local-next-step-12345678"
        )
        .is_ok());
    }

    #[test]
    fn rejects_unsafe_patch_paths() {
        let mut patch = patch_plan();
        patch.edits[0].path = "../AGENTS.md".to_string();
        assert!(validate_patch_plan(
            &patch,
            &task(),
            "agent/oneepis-local-next-step-12345678"
        )
        .is_err());
    }

    #[test]
    fn rejects_unexpected_branch() {
        let mut patch = patch_plan();
        patch.branch_name = "main".to_string();
        assert!(validate_patch_plan(
            &patch,
            &task(),
            "agent/oneepis-local-next-step-12345678"
        )
        .is_err());
    }
}
