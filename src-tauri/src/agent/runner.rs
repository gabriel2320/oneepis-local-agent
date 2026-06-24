use crate::agent::brief::{
    build_development_brief, build_implementation_decision, development_brief,
};
use crate::agent::context_pack::build_development_context_pack;
use crate::agent::evolution::evolution_plan;
use crate::agent::governance::apply_oneepis_governance;
use crate::agent::ollama::ask_for_micro_plan;
use crate::agent::persistence::record_run;
use crate::agent::repo::inspect_repository;
use crate::agent::safety::{sanitize_log, sha256_hex};
use crate::agent::types::{
    AgentRun, AgentRunReport, AgentStep, DevelopmentBrief, DevelopmentContextPack,
    DevelopmentWorkPackage, EvolutionPlan, ImplementationDecision, MicroPlan, RunRequest,
};
use crate::agent::work_package::development_work_package;
use chrono::Utc;
use std::path::Path;

const STATES: &[&str] = &[
    "preflight",
    "governance_read",
    "repo_audit",
    "evolution_plan",
    "work_package",
    "context_pack",
    "development_brief",
    "implementation_decision",
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
        normalize_plan(&mut plan, &inspection);
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
    let apply_requested = request.allow_apply
        || request.confirm_token.is_some()
        || request.branch_strategy != "reuse";
    let started_at = Utc::now().to_rfc3339();
    let inspection = inspect_repository(&request.repo_path)?;
    let evolution = evolution_plan(&request.repo_path, &request.objective, None).await?;
    let package = development_work_package(&request.repo_path, &request.objective, None).await?;
    let context = build_development_context_pack(Path::new(&inspection.repo_path), &package);
    let brief = if request.ask_model {
        development_brief(&request.repo_path, &request.objective, true, None).await?
    } else {
        build_development_brief(&package, &context, None)
    };
    let decision = build_implementation_decision(&brief);
    let plan = plan_microcycle(&request.repo_path, &request.objective, None).await?;
    let blocked = plan.blocked
        || package.status == "blocked"
        || context.status == "blocked"
        || brief.status == "blocked"
        || decision.status == "blocked"
        || evolution.status == "blocked"
        || !inspection.blocks.is_empty()
        || mode != "dry_run"
        || apply_requested;
    let mut steps = Vec::new();

    for (index, state) in STATES.iter().enumerate() {
        let status = if *state == "evolution_plan" && evolution.status == "blocked" {
            "blocked"
        } else if *state == "implementation_decision" && decision.status == "blocked" {
            "blocked"
        } else {
            state_status(state, blocked, &mode)
        };
        steps.push(AgentStep {
            order: index + 1,
            state: state.to_string(),
            status: status.to_string(),
            summary: state_summary(
                state,
                &inspection,
                &package,
                &context,
                &evolution,
                &brief,
                &decision,
                &plan,
                max_cycles,
                &mode,
            ),
        });
        if blocked && *state == "safety_review" {
            break;
        }
    }

    let mut lessons = vec![
        "Registrar primero el contexto y la gobernanza reduce cambios innecesarios.".to_string(),
        "La primera version ejecuta dry-run: los patches reales quedan bloqueados hasta v0.3."
            .to_string(),
    ];
    if inspection.is_one_epis {
        lessons.push(
            "OneEpis requiere microciclos pequenos y gates oficiales antes de crecer.".to_string(),
        );
    }
    lessons.push(format!(
        "Evolucion={}, paquete={}, contexto={}, brief={}, decision={}.",
        evolution.status, package.status, context.status, brief.status, decision.status
    ));
    if max_cycles > 1 {
        lessons.push(format!(
            "Se pidieron {max_cycles} ciclos, pero v0.1 registra solo una pasada segura."
        ));
    }
    if apply_requested {
        lessons.push(
            "El runner dry-run no aplica cambios; usa apply_approved_patch con PatchDraft aprobado."
                .to_string(),
        );
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

pub async fn run_microcycle_report(request: RunRequest) -> Result<AgentRunReport, String> {
    let run = run_microcycle(request).await?;
    Ok(build_run_report(&run))
}

pub fn build_run_report(run: &AgentRun) -> AgentRunReport {
    let blocked_steps = run
        .steps
        .iter()
        .filter(|step| step.status == "blocked" || step.status == "failed")
        .collect::<Vec<_>>();
    let verdict = if run.status == "completed" && blocked_steps.is_empty() {
        "ready_for_review"
    } else {
        "blocked_or_needs_review"
    };
    let checklist = vec![
        format!("Objetivo acotado: {}", run.objective),
        format!("Modo sin escritura: {}", run.mode),
        format!("Gate recomendado: {}", run.plan.recommended_gate),
        format!("Riesgo: {}", run.plan.risk_level),
        format!("Persistencia: {}", run.persistence),
    ];
    let mut warnings = run.plan.warnings.clone();
    warnings.extend(
        blocked_steps
            .iter()
            .map(|step| format!("{}: {}", step.state, step.summary)),
    );
    let next_actions = if verdict == "ready_for_review" {
        vec![
            format!("Ejecutar {} si aplica al PR.", run.plan.recommended_gate),
            "Convertir una sola decision aprobada en PatchDraft revisable.".to_string(),
            "Registrar resultado en la descripcion del PR.".to_string(),
        ]
    } else {
        vec![
            "Resolver bloqueos antes de PatchDraft.".to_string(),
            "Regenerar paquete, contexto y brief tras resolverlos.".to_string(),
            "No aplicar cambios reales desde este reporte.".to_string(),
        ]
    };
    let markdown = run_report_markdown(run, verdict, &checklist, &warnings, &next_actions);

    AgentRunReport {
        run_id: run.id.clone(),
        status: run.status.clone(),
        verdict: verdict.to_string(),
        objective: run.objective.clone(),
        branch: run.branch.clone(),
        model_used: run.model_used.clone(),
        recommended_gate: run.plan.recommended_gate.clone(),
        markdown,
        checklist,
        warnings,
        next_actions,
    }
}

fn fallback_plan(inspection: &crate::agent::types::RepoInspection, objective: &str) -> MicroPlan {
    let recommended_gate = select_gate(&inspection.declared_gates);
    let mut warnings = inspection.blocks.clone();
    if !inspection.is_one_epis {
        warnings.push(
            "Repo generico: se aplican reglas basicas de safety, no doctrina OneEpis completa."
                .to_string(),
        );
    }
    let mut plan = MicroPlan {
        objective: sanitize_log(objective),
        recommended_gate: recommended_gate.clone(),
        risk_level: if inspection.blocks.is_empty() {
            "green".to_string()
        } else {
            "yellow".to_string()
        },
        touched_surfaces: vec!["governance".to_string(), "repo".to_string()],
        required_gates: if recommended_gate == "sin_gate" {
            Vec::new()
        } else {
            vec![recommended_gate]
        },
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
    };
    normalize_plan(&mut plan, inspection);
    plan
}

fn normalize_plan(plan: &mut MicroPlan, inspection: &crate::agent::types::RepoInspection) {
    if inspection.is_one_epis {
        apply_oneepis_governance(plan, inspection);
    }
    if let Some(preferred_gate) =
        preferred_gate_for_objective(&plan.objective, &inspection.declared_gates)
    {
        plan.recommended_gate = preferred_gate.clone();
        if !plan
            .required_gates
            .iter()
            .any(|gate| gate == &preferred_gate)
        {
            plan.required_gates.push(preferred_gate);
        }
    }
    if plan.recommended_gate.is_empty()
        || !inspection.declared_gates.contains(&plan.recommended_gate)
    {
        plan.recommended_gate = plan
            .required_gates
            .first()
            .cloned()
            .unwrap_or_else(|| select_gate(&inspection.declared_gates));
    }
    if plan.risk_level.is_empty() {
        plan.risk_level = if plan.blocked { "yellow" } else { "green" }.to_string();
    }
    if !matches!(plan.risk_level.as_str(), "green" | "yellow" | "red") {
        plan.risk_level = "yellow".to_string();
    }
    if plan.required_gates.is_empty() && plan.recommended_gate != "sin_gate" {
        plan.required_gates = vec![plan.recommended_gate.clone()];
    }
    plan.required_gates
        .retain(|gate| inspection.declared_gates.contains(gate));
    if plan.touched_surfaces.is_empty() {
        plan.touched_surfaces = vec!["repo".to_string()];
    }
    if plan.steps.is_empty() {
        plan.steps = vec!["Reducir el objetivo a un microciclo verificable.".to_string()];
    }
    let hard_blocked = !inspection.blocks.is_empty() || plan.risk_level == "red";
    if plan.blocked && !hard_blocked {
        plan.blocked = false;
        plan.warnings.push(
            "El modelo marco bloqueo sin bloqueo duro; se trata como advertencia en dry-run gobernado."
                .to_string(),
        );
    }
}

fn select_gate(gates: &[String]) -> String {
    for preferred in [
        "check:size",
        "check:api",
        "check:web",
        "check",
        "test",
        "build",
    ] {
        if gates.iter().any(|gate| gate == preferred) {
            return preferred.to_string();
        }
    }
    gates
        .first()
        .cloned()
        .unwrap_or_else(|| "sin_gate".to_string())
}

fn preferred_gate_for_objective(objective: &str, gates: &[String]) -> Option<String> {
    let objective = objective.to_ascii_lowercase();
    let has_gate = |gate: &str| gates.iter().any(|known| known == gate);
    if has_gate("check:size")
        && ["size", "tamano", "near-limit", "archivo"]
            .iter()
            .any(|needle| objective.contains(needle))
    {
        return Some("check:size".to_string());
    }
    if has_gate("check:contract")
        && ["contrato", "openapi"]
            .iter()
            .any(|needle| objective.contains(needle))
    {
        return Some("check:contract".to_string());
    }
    if has_gate("check:web")
        && ["web", "pantalla", "ruta", "screen"]
            .iter()
            .any(|needle| objective.contains(needle))
    {
        return Some("check:web".to_string());
    }
    if has_gate("check:api")
        && ["api", "endpoint", "postgres"]
            .iter()
            .any(|needle| objective.contains(needle))
    {
        return Some("check:api".to_string());
    }
    None
}

fn state_status(state: &str, blocked: bool, mode: &str) -> &'static str {
    if blocked && state == "safety_review" {
        return "blocked";
    }
    if blocked
        && matches!(
            state,
            "apply_patch" | "gate_run" | "result_record" | "lesson_record" | "stop_or_next"
        )
    {
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
    package: &DevelopmentWorkPackage,
    context: &DevelopmentContextPack,
    evolution: &EvolutionPlan,
    brief: &DevelopmentBrief,
    decision: &ImplementationDecision,
    plan: &MicroPlan,
    max_cycles: u8,
    mode: &str,
) -> String {
    match state {
        "preflight" => format!(
            "Repo {} en rama {}.",
            inspection.project_name, inspection.current_branch
        ),
        "governance_read" => format!(
            "{} documentos de gobernanza presentes.",
            inspection
                .governance_documents
                .iter()
                .filter(|doc| doc.present)
                .count()
        ),
        "repo_audit" => {
            if inspection.dirty {
                "Worktree sucio; bloqueo preventivo activado.".to_string()
            } else {
                "Worktree limpio para planificar.".to_string()
            }
        }
        "evolution_plan" => {
            if let Some(candidate) = &evolution.selected_candidate {
                let score = evolution
                    .ranked_candidates
                    .iter()
                    .find(|item| item.candidate.id == candidate.id)
                    .map(|item| item.score.net_score)
                    .unwrap_or_default();
                format!(
                    "Evolucion {}: {} con score {} y gates {}.",
                    evolution.status,
                    candidate.title,
                    score,
                    join_or_empty(&candidate.gates)
                )
            } else {
                format!(
                    "Evolucion {} sin candidato ejecutable; bloqueos: {}.",
                    evolution.status,
                    evolution.blockers.len()
                )
            }
        }
        "work_package" => format!(
            "Paquete {} con {} archivo(s) a inspeccionar y gates: {}.",
            package.status,
            package.files_to_inspect.len(),
            join_or_empty(&package.gates)
        ),
        "context_pack" => format!(
            "Contexto {} con {} entrada(s), {}/{} bytes y {} warning(s).",
            context.status,
            context.files.len(),
            context.total_bytes,
            context.max_bytes,
            context.warnings.len()
        ),
        "development_brief" => format!(
            "Brief {} para modelo local; propuesta: {}.",
            brief.status,
            brief
                .proposal
                .as_ref()
                .map(|proposal| proposal.status.as_str())
                .unwrap_or("no solicitada")
        ),
        "implementation_decision" => format!(
            "Decision {} desde propuesta {}; {} archivo(s), gates: {}, bloqueos: {}.",
            decision.status,
            decision.source_proposal_status,
            decision.selected_files.len(),
            join_or_empty(&decision.required_gates),
            decision.blockers.len()
        ),
        "micro_plan" => format!(
            "Plan generado con modelo {}; gate recomendado {}.",
            plan.model_used, plan.recommended_gate
        ),
        "patch_draft" => {
            "v0.2 genera PatchDraft revisable sin escribir en el repo objetivo.".to_string()
        }
        "safety_review" => {
            if mode != "dry_run" {
                "Modo controlado bloqueado hasta v0.3.".to_string()
            } else if decision.status == "blocked" {
                "Decision de implementacion bloqueada; no se prepara PatchDraft/apply.".to_string()
            } else if plan.blocked {
                "Plan bloqueado por gobernanza o estado del repo.".to_string()
            } else {
                "Safety kernel permite dry-run sin escritura.".to_string()
            }
        }
        "apply_patch" => "No se aplica patch en dry-run.".to_string(),
        "gate_run" => format!(
            "Gate recomendado para ejecucion futura: {}.",
            plan.recommended_gate
        ),
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

fn run_report_markdown(
    run: &AgentRun,
    verdict: &str,
    checklist: &[String],
    warnings: &[String],
    next_actions: &[String],
) -> String {
    let mut lines = vec![
        "# OneEpis Local Agent Microprocess Report".to_string(),
        String::new(),
        format!("- Run: {}", run.id),
        format!("- Objective: {}", run.objective),
        format!("- Status: {}", run.status),
        format!("- Verdict: {verdict}"),
        format!("- Mode: {}", run.mode),
        format!("- Branch: {}", run.branch),
        format!("- Model: {}", run.model_used),
        format!("- Recommended gate: {}", run.plan.recommended_gate),
        String::new(),
        "## States".to_string(),
    ];
    lines.extend(run.steps.iter().map(|step| {
        format!(
            "- {}. {}: {} - {}",
            step.order, step.state, step.status, step.summary
        )
    }));
    lines.push(String::new());
    lines.push("## Checklist".to_string());
    lines.extend(checklist.iter().map(|item| format!("- {item}")));
    lines.push(String::new());
    lines.push("## Warnings".to_string());
    if warnings.is_empty() {
        lines.push("- Sin warnings.".to_string());
    } else {
        lines.extend(warnings.iter().map(|item| format!("- {item}")));
    }
    lines.push(String::new());
    lines.push("## Next Actions".to_string());
    lines.extend(next_actions.iter().map(|item| format!("- {item}")));
    lines.push(String::new());
    lines.push("## Lessons".to_string());
    lines.extend(run.lessons.iter().map(|item| format!("- {item}")));
    sanitize_log(&lines.join("\n"))
}

fn join_or_empty(items: &[String]) -> String {
    if items.is_empty() {
        "sin_gate".to_string()
    } else {
        items.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runner_states_include_context_brief_and_decision_before_patch() {
        let audit_index = STATES
            .iter()
            .position(|state| *state == "repo_audit")
            .expect("audit state");
        let evolution_index = STATES
            .iter()
            .position(|state| *state == "evolution_plan")
            .expect("evolution state");
        let work_index = STATES
            .iter()
            .position(|state| *state == "work_package")
            .expect("work package state");
        let context_index = STATES
            .iter()
            .position(|state| *state == "context_pack")
            .expect("context state");
        let brief_index = STATES
            .iter()
            .position(|state| *state == "development_brief")
            .expect("brief state");
        let decision_index = STATES
            .iter()
            .position(|state| *state == "implementation_decision")
            .expect("decision state");
        let patch_index = STATES
            .iter()
            .position(|state| *state == "patch_draft")
            .expect("patch state");

        assert!(audit_index < evolution_index);
        assert!(evolution_index < work_index);
        assert!(work_index < context_index);
        assert!(context_index < brief_index);
        assert!(brief_index < decision_index);
        assert!(decision_index < patch_index);
    }

    #[test]
    fn blocked_runner_still_reaches_safety_review() {
        assert_eq!(state_status("context_pack", true, "dry_run"), "completed");
        assert_eq!(
            state_status("development_brief", true, "dry_run"),
            "completed"
        );
        assert_eq!(
            state_status("implementation_decision", true, "dry_run"),
            "completed"
        );
        assert_eq!(state_status("safety_review", true, "dry_run"), "blocked");
        assert_eq!(state_status("gate_run", true, "dry_run"), "skipped");
    }

    #[test]
    fn size_objectives_prefer_size_gate() {
        let gates = vec![
            "check:api".to_string(),
            "check:size".to_string(),
            "check:web".to_string(),
        ];

        assert_eq!(
            preferred_gate_for_objective("Reducir archivo clinico near-limit", &gates),
            Some("check:size".to_string())
        );
    }

    #[test]
    fn run_report_contains_pr_ready_sections() {
        let run = AgentRun {
            id: "run-test".to_string(),
            repo_path: "C:\\OneEpis".to_string(),
            objective: "Reducir archivo".to_string(),
            branch: "main".to_string(),
            status: "completed".to_string(),
            mode: "dry_run".to_string(),
            model_used: "local_rules".to_string(),
            started_at: "start".to_string(),
            completed_at: "end".to_string(),
            steps: vec![AgentStep {
                order: 1,
                state: "work_package".to_string(),
                status: "completed".to_string(),
                summary: "Paquete listo.".to_string(),
            }],
            plan: MicroPlan {
                objective: "Reducir archivo".to_string(),
                recommended_gate: "check:size".to_string(),
                risk_level: "green".to_string(),
                touched_surfaces: vec!["repo".to_string()],
                required_gates: vec!["check:size".to_string()],
                steps: vec!["Leer contexto.".to_string()],
                warnings: Vec::new(),
                blocked: false,
                model_used: "local_rules".to_string(),
            },
            lessons: vec!["Cerrar con gate.".to_string()],
            persistence: "not_configured".to_string(),
        };

        let report = build_run_report(&run);

        assert_eq!(report.verdict, "ready_for_review");
        assert!(report.markdown.contains("## States"));
        assert!(report.markdown.contains("## Next Actions"));
        assert!(report.recommended_gate.contains("check:size"));
    }
}
