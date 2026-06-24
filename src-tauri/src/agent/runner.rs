use crate::agent::ollama::ask_for_micro_plan;
use crate::agent::persistence::record_run;
use crate::agent::repo::inspect_repository;
use crate::agent::safety::{sanitize_log, sha256_hex};
use crate::agent::types::{AgentRun, AgentStep, MicroPlan, RunRequest};
use chrono::Utc;

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

