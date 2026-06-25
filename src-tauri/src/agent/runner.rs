use crate::agent::ollama::ask_for_micro_plan;
use crate::agent::persistence::record_run;
use crate::agent::repo::{ensure_oneepis_checkout, inspect_repository};
use crate::agent::safety::{sanitize_log, sha256_hex};
use crate::agent::types::{AgentRun, AgentStep, AutopilotRequest, MicroPlan, NextWork, RunRequest};
use chrono::Utc;
use std::path::Path;
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
        lessons,
        persistence: "pending".to_string(),
    };

    run.persistence = match record_run(request.database_url, &run).await {
        Ok(status) => status,
        Err(err) => format!("not_recorded: {}", sanitize_log(&err)),
    };
    Ok(run)
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
