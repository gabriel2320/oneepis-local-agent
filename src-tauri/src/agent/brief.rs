use crate::agent::context_pack::build_development_context_pack;
use crate::agent::ollama::ask_for_development_proposal;
use crate::agent::repo::inspect_repository;
use crate::agent::safety::sanitize_log;
use crate::agent::types::{
    DevelopmentBrief, DevelopmentContextPack, DevelopmentWorkPackage, ImplementationDecision,
    LocalModelProposal,
};
use crate::agent::work_package::{build_development_work_package, development_work_package};
use std::path::Path;

const MAX_PROMPT_CONTEXT_BYTES: usize = 14 * 1024;

pub async fn development_brief(
    repo_path: &str,
    objective: &str,
    ask_model: bool,
    base_url: Option<String>,
) -> Result<DevelopmentBrief, String> {
    let package = development_work_package(repo_path, objective, base_url.clone()).await?;
    let context = build_development_context_pack(Path::new(repo_path), &package);
    let brief = build_development_brief(&package, &context, None);

    if ask_model && brief.status != "blocked" {
        let mut proposal =
            ask_for_development_proposal(base_url, &brief.system_prompt, &brief.user_prompt).await;
        validate_model_proposal(&mut proposal, &package, &context);
        return Ok(build_development_brief(&package, &context, Some(proposal)));
    }

    Ok(brief)
}

pub async fn implementation_decision(
    repo_path: &str,
    objective: &str,
    ask_model: bool,
    base_url: Option<String>,
) -> Result<ImplementationDecision, String> {
    let brief = development_brief(repo_path, objective, ask_model, base_url).await?;
    Ok(build_implementation_decision(&brief))
}

pub fn build_development_brief(
    package: &DevelopmentWorkPackage,
    context: &DevelopmentContextPack,
    proposal: Option<LocalModelProposal>,
) -> DevelopmentBrief {
    let context_files = context
        .files
        .iter()
        .map(|file| format!("{} ({})", file.path, file.kind))
        .collect::<Vec<_>>();
    let mut warnings = package.warnings.clone();
    warnings.extend(context.warnings.clone());
    if proposal
        .as_ref()
        .map(|item| item.status.as_str() != "proposed")
        .unwrap_or(false)
    {
        warnings.push("Propuesta local requiere revision humana antes de PatchDraft.".to_string());
    }
    warnings = dedupe(warnings);

    let status = if package.status == "blocked" || context.status == "blocked" {
        "blocked"
    } else if context.status == "partial" || !warnings.is_empty() {
        "partial"
    } else {
        "ready"
    };
    let model_used = proposal
        .as_ref()
        .map(|item| item.model_used.clone())
        .unwrap_or_else(|| "brief_only".to_string());
    let system_prompt = system_prompt();
    let user_prompt = user_prompt(package, context);

    DevelopmentBrief {
        repo_path: package.repo_path.clone(),
        objective: package.objective.clone(),
        status: status.to_string(),
        summary: summary_for(status, package, context, warnings.len()),
        model_used,
        work_order: work_order(package, context),
        system_prompt,
        user_prompt,
        response_contract: response_contract(),
        context_files,
        gates: package.gates.clone(),
        warnings,
        stop_conditions: package.stop_conditions.clone(),
        next_actions: next_actions(status, package),
        proposal,
    }
}

pub fn build_implementation_decision(brief: &DevelopmentBrief) -> ImplementationDecision {
    let proposal_status = brief
        .proposal
        .as_ref()
        .map(|proposal| proposal.status.clone())
        .unwrap_or_else(|| "missing".to_string());
    let mut blockers = Vec::new();
    let mut warnings = brief.warnings.clone();
    let mut selected_files = Vec::new();
    let mut implementation_steps = Vec::new();
    let mut required_gates = brief.gates.clone();
    let mut patch_intent = "Sin decision aprobada para PatchDraft.".to_string();

    if brief.status == "blocked" {
        blockers.push(
            "Brief bloqueado: resolver paquete/contexto antes de decidir implementacion."
                .to_string(),
        );
    }

    match brief.proposal.as_ref() {
        Some(proposal) if proposal.status == "proposed" => {
            selected_files = proposal.files_to_change.clone();
            implementation_steps = proposal.implementation_notes.clone();
            required_gates = if proposal.gates.is_empty() {
                brief.gates.clone()
            } else {
                proposal.gates.clone()
            };
            patch_intent = format!(
                "Convertir la propuesta local en un PatchDraft limitado a {} archivo(s) y {} gate(s).",
                selected_files.len(),
                required_gates.len()
            );
            if selected_files.is_empty() {
                blockers.push("La propuesta no selecciona archivos para cambiar.".to_string());
            }
            if implementation_steps.is_empty() {
                blockers
                    .push("La propuesta no deja pasos de implementacion revisables.".to_string());
            }
            if required_gates.is_empty() {
                blockers.push("La decision no declara gates requeridos.".to_string());
            }
            warnings.extend(proposal.risks.clone());
        }
        Some(proposal) => {
            blockers.push(format!(
                "Propuesta local no aprobable para PatchDraft: {}.",
                proposal.status
            ));
            warnings.extend(proposal.risks.clone());
        }
        None => {
            blockers.push(
                "Falta propuesta del modelo local; pedir Brief IA antes de decidir implementacion."
                    .to_string(),
            );
        }
    }

    let blockers = dedupe(blockers);
    let warnings = dedupe(warnings);
    let status = if brief.status == "blocked" {
        "blocked"
    } else if !blockers.is_empty() {
        if proposal_status == "missing" {
            "needs_model_proposal"
        } else {
            "blocked"
        }
    } else {
        "ready_to_draft"
    };

    ImplementationDecision {
        repo_path: brief.repo_path.clone(),
        objective: brief.objective.clone(),
        status: status.to_string(),
        summary: decision_summary(status, brief, &selected_files, &required_gates),
        model_used: brief.model_used.clone(),
        source_proposal_status: proposal_status,
        selected_files,
        implementation_steps,
        required_gates,
        acceptance_criteria: decision_acceptance_criteria(brief),
        blockers: blockers.clone(),
        warnings,
        patch_intent,
        next_actions: decision_next_actions(status, &blockers),
    }
}

#[allow(dead_code)]
pub fn deterministic_brief_from_objective(
    repo_path: &str,
    objective: &str,
) -> Result<DevelopmentBrief, String> {
    let inspection = inspect_repository(repo_path)?;
    let readiness = crate::agent::readiness::build_development_readiness(
        &inspection,
        &crate::agent::types::OllamaStatus {
            base_url: "local_rules".to_string(),
            available: false,
            message: "Brief determinista sin llamada a Ollama.".to_string(),
            models: Vec::new(),
            policy: Default::default(),
            missing_policy_models: Vec::new(),
        },
    );
    let package = build_development_work_package(&inspection, &readiness, objective);
    let context = build_development_context_pack(Path::new(repo_path), &package);
    Ok(build_development_brief(&package, &context, None))
}

fn system_prompt() -> String {
    [
        "Eres el asistente local de programacion de OneEpis dentro de OneEpis Local Agent.",
        "Usas solo modelos Ollama locales y trabajas sobre un brief gobernado.",
        "No diagnosticas, no firmas, no recetas, no ordenas acciones clinicas y no inventas datos.",
        "No escribes archivos ni pides apply; entregas una propuesta tecnica revisable.",
        "Respeta la escalera: paciente -> ficha -> papel -> API -> PostgreSQL -> auditoria -> permisos -> OpenAPI.",
        "Devuelve solo JSON compacto con summary, filesToChange, implementationNotes, risks y gates.",
    ]
    .join("\n")
}

fn user_prompt(package: &DevelopmentWorkPackage, context: &DevelopmentContextPack) -> String {
    let mut lines = vec![
        format!("Objetivo: {}", sanitize_log(&package.objective)),
        format!("Estado paquete: {}", package.status),
        format!("Gates requeridos: {}", join_or_empty(&package.gates)),
        format!("Estrategia de rama futura: {}", package.branch_strategy),
        String::new(),
        "Pasos de implementacion esperados:".to_string(),
    ];
    lines.extend(
        package
            .implementation_steps
            .iter()
            .map(|step| format!("- {step}")),
    );
    lines.push(String::new());
    lines.push("Criterios de aceptacion:".to_string());
    lines.extend(
        package
            .acceptance_criteria
            .iter()
            .map(|criterion| format!("- {criterion}")),
    );
    lines.push(String::new());
    lines.push("Contexto local sanitizado:".to_string());
    lines.push(context_excerpt(context));
    lines.push(String::new());
    lines.push("Respuesta obligatoria: JSON con microdiff conceptual, sin diff aplicable y sin comandos libres.".to_string());
    sanitize_log(&lines.join("\n"))
}

fn context_excerpt(context: &DevelopmentContextPack) -> String {
    let mut output = String::new();
    for file in &context.files {
        let header = format!(
            "\n### {} [{}; {} bytes; {} lines]\n{}\n",
            file.path, file.kind, file.bytes, file.lines, file.summary
        );
        if output.len() + header.len() > MAX_PROMPT_CONTEXT_BYTES {
            output.push_str("\n[CONTEXT_BUDGET_EXHAUSTED]");
            break;
        }
        output.push_str(&header);
        if !file.excerpt.is_empty() {
            let remaining = MAX_PROMPT_CONTEXT_BYTES.saturating_sub(output.len());
            if remaining == 0 {
                output.push_str("\n[CONTEXT_BUDGET_EXHAUSTED]");
                break;
            }
            output.push_str(&take_chars(&file.excerpt, remaining));
            output.push('\n');
        }
    }
    output
}

fn take_chars(input: &str, max_bytes: usize) -> String {
    if input.len() <= max_bytes {
        return input.to_string();
    }
    let mut output = String::new();
    for ch in input.chars() {
        if output.len() + ch.len_utf8() > max_bytes.saturating_sub(13) {
            break;
        }
        output.push(ch);
    }
    output.push_str("\n[TRUNCATED]");
    output
}

fn work_order(package: &DevelopmentWorkPackage, context: &DevelopmentContextPack) -> String {
    format!(
        "Preparar propuesta local para '{}'. Usar {} entradas de contexto, no escribir archivos, proponer solo cambios pequenos y validar con {}.",
        package.objective,
        context.files.len(),
        join_or_empty(&package.gates)
    )
}

fn summary_for(
    status: &str,
    package: &DevelopmentWorkPackage,
    context: &DevelopmentContextPack,
    warning_count: usize,
) -> String {
    match status {
        "blocked" => format!(
            "Brief bloqueado para '{}'; resolver readiness/contexto antes de pedir propuesta al modelo.",
            package.objective
        ),
        "partial" => format!(
            "Brief parcial para '{}': {} archivos de contexto y {} warning(s).",
            package.objective,
            context.files.len(),
            warning_count
        ),
        _ => format!(
            "Brief listo para '{}': {} archivos de contexto y gates {}.",
            package.objective,
            context.files.len(),
            join_or_empty(&package.gates)
        ),
    }
}

fn response_contract() -> Vec<String> {
    vec![
        "summary: una frase de cambio propuesto, sin prometer apply.".to_string(),
        "filesToChange: rutas relativas existentes o nuevas solo si gobernanza lo permite."
            .to_string(),
        "implementationNotes: pasos pequenos que un humano puede revisar.".to_string(),
        "risks: riesgos, omisiones o contexto faltante.".to_string(),
        "gates: gates oficiales declarados que deben pasar.".to_string(),
    ]
}

fn next_actions(status: &str, package: &DevelopmentWorkPackage) -> Vec<String> {
    if status == "blocked" {
        return vec![
            "Resolver blockers de readiness o contexto.".to_string(),
            "Regenerar paquete/contexto antes de pedir propuesta al modelo.".to_string(),
            "No crear PatchDraft hasta tener brief revisable.".to_string(),
        ];
    }
    let mut actions = vec![
        "Revisar el brief y warnings.".to_string(),
        "Pedir propuesta al modelo local si Ollama esta disponible.".to_string(),
        "Convertir solo una decision aprobada en PatchDraft revisable.".to_string(),
    ];
    for gate in &package.gates {
        actions.push(format!("Validar con {gate} antes de cerrar el ciclo."));
    }
    actions
}

fn decision_summary(
    status: &str,
    brief: &DevelopmentBrief,
    selected_files: &[String],
    gates: &[String],
) -> String {
    match status {
        "ready_to_draft" => format!(
            "Decision lista para PatchDraft: {} archivo(s), gates {}.",
            selected_files.len(),
            join_or_empty(gates)
        ),
        "needs_model_proposal" => format!(
            "Decision pendiente para '{}': falta propuesta local revisable.",
            brief.objective
        ),
        _ => format!(
            "Decision bloqueada para '{}': revisar propuesta, contexto o gobernanza.",
            brief.objective
        ),
    }
}

fn decision_acceptance_criteria(brief: &DevelopmentBrief) -> Vec<String> {
    let mut criteria = vec![
        "La decision se convierte en un solo PatchDraft revisable.".to_string(),
        "No se escribe en el repo objetivo desde la decision.".to_string(),
        "No se agregan secretos, PHI ni identificadores reales.".to_string(),
    ];
    criteria.extend(
        brief
            .gates
            .iter()
            .map(|gate| format!("{gate} debe pasar o el microciclo se detiene.")),
    );
    criteria
}

fn decision_next_actions(status: &str, blockers: &[String]) -> Vec<String> {
    match status {
        "ready_to_draft" => vec![
            "Crear PatchDraft usando solo la decision seleccionada.".to_string(),
            "Revisar diff, safety y ApplyReadiness antes de escribir.".to_string(),
            "Ejecutar el gate requerido tras cualquier apply controlado.".to_string(),
        ],
        "needs_model_proposal" => vec![
            "Presionar Brief IA para pedir propuesta al modelo local.".to_string(),
            "Revisar que archivos y gates vengan desde el contexto gobernado.".to_string(),
            "No crear PatchDraft hasta tener una decision ready_to_draft.".to_string(),
        ],
        _ => blockers
            .iter()
            .take(3)
            .cloned()
            .chain(["Regenerar contexto/brief antes de PatchDraft.".to_string()])
            .collect(),
    }
}

fn join_or_empty(items: &[String]) -> String {
    if items.is_empty() {
        "sin_gate".to_string()
    } else {
        items.join(", ")
    }
}

fn validate_model_proposal(
    proposal: &mut LocalModelProposal,
    package: &DevelopmentWorkPackage,
    context: &DevelopmentContextPack,
) {
    if proposal.status != "proposed" {
        return;
    }

    let context_paths = context
        .files
        .iter()
        .map(|file| normalize_path(&file.path))
        .collect::<Vec<_>>();
    let context_dirs = context
        .files
        .iter()
        .filter(|file| file.kind == "directory")
        .map(|file| normalize_path(&file.path))
        .collect::<Vec<_>>();
    let requested_dirs = package
        .files_to_inspect
        .iter()
        .filter(|path| !path.contains('.'))
        .map(|path| normalize_path(path))
        .collect::<Vec<_>>();

    for path in &proposal.files_to_change {
        let normalized = normalize_path(path);
        let known_path = context_paths.iter().any(|item| item == &normalized)
            || context_dirs
                .iter()
                .chain(requested_dirs.iter())
                .any(|dir| normalized.starts_with(&format!("{dir}/")));
        if !known_path {
            proposal.risks.push(format!(
                "Ruta propuesta fuera del contexto gobernado: {normalized}."
            ));
        }
    }

    for gate in &proposal.gates {
        if !package.gates.iter().any(|known| known == gate) {
            proposal.risks.push(format!(
                "Gate propuesto no declarado por el paquete: {gate}."
            ));
        }
    }

    if proposal.implementation_notes.is_empty() {
        proposal
            .risks
            .push("Propuesta sin pasos de implementacion revisables.".to_string());
    }

    if !proposal.risks.is_empty() {
        proposal.status = "needs_review".to_string();
    }
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn dedupe(items: Vec<String>) -> Vec<String> {
    let mut deduped = Vec::new();
    for item in items {
        if !deduped.iter().any(|candidate| candidate == &item) {
            deduped.push(item);
        }
    }
    deduped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::{ContextPackFile, WorkPackageTest};

    #[test]
    fn brief_contains_governed_prompt_contract_and_context() {
        let package = package("ready_to_draft");
        let context = context("ready");
        let brief = build_development_brief(&package, &context, None);

        assert_eq!(brief.status, "ready");
        assert!(brief.system_prompt.contains("solo modelos Ollama locales"));
        assert!(brief.system_prompt.contains("No escribes archivos"));
        assert!(brief.user_prompt.contains("AGENTS.md"));
        assert!(brief.user_prompt.contains("Respuesta obligatoria"));
        assert!(brief
            .response_contract
            .iter()
            .any(|item| item.contains("filesToChange")));
        assert_eq!(brief.model_used, "brief_only");
    }

    #[test]
    fn blocked_context_blocks_model_proposal_path() {
        let package = package("blocked");
        let context = context("blocked");
        let brief = build_development_brief(&package, &context, None);

        assert_eq!(brief.status, "blocked");
        assert!(brief
            .next_actions
            .iter()
            .any(|action| action.contains("No crear PatchDraft")));
    }

    #[test]
    fn proposal_keeps_model_identity_and_warnings() {
        let package = package("ready_to_draft");
        let context = context("partial");
        let proposal = LocalModelProposal {
            status: "proposed".to_string(),
            model_used: "qwen3:8b".to_string(),
            summary: "Extraer helper pequeno.".to_string(),
            files_to_change: vec!["src/file.ts".to_string()],
            implementation_notes: vec!["Mover regla pura.".to_string()],
            risks: Vec::new(),
            gates: vec!["check:size".to_string()],
            raw_response: "{}".to_string(),
        };
        let brief = build_development_brief(&package, &context, Some(proposal));

        assert_eq!(brief.status, "partial");
        assert_eq!(brief.model_used, "qwen3:8b");
        assert!(brief.proposal.is_some());
    }

    #[test]
    fn proposal_outside_context_requires_review() {
        let package = package("ready_to_draft");
        let context = context("ready");
        let mut proposal = LocalModelProposal {
            status: "proposed".to_string(),
            model_used: "qwen3:8b".to_string(),
            summary: "Cambiar ruta inventada.".to_string(),
            files_to_change: vec!["apps/api/src/models/ClinicalRecord.ts".to_string()],
            implementation_notes: vec!["Editar archivo.".to_string()],
            risks: Vec::new(),
            gates: vec!["check:size".to_string()],
            raw_response: "{}".to_string(),
        };

        validate_model_proposal(&mut proposal, &package, &context);

        assert_eq!(proposal.status, "needs_review");
        assert!(proposal
            .risks
            .iter()
            .any(|risk| risk.contains("fuera del contexto")));
    }

    #[test]
    fn decision_ready_to_draft_from_governed_proposal() {
        let package = package("ready_to_draft");
        let context = context("ready");
        let proposal = LocalModelProposal {
            status: "proposed".to_string(),
            model_used: "qwen3:8b".to_string(),
            summary: "Actualizar guia gobernada.".to_string(),
            files_to_change: vec!["AGENTS.md".to_string()],
            implementation_notes: vec!["Agregar criterio pequeno.".to_string()],
            risks: Vec::new(),
            gates: vec!["check:size".to_string()],
            raw_response: "{}".to_string(),
        };
        let brief = build_development_brief(&package, &context, Some(proposal));

        let decision = build_implementation_decision(&brief);

        assert_eq!(decision.status, "ready_to_draft");
        assert_eq!(decision.source_proposal_status, "proposed");
        assert!(decision.selected_files.contains(&"AGENTS.md".to_string()));
        assert!(decision
            .next_actions
            .iter()
            .any(|action| action.contains("PatchDraft")));
    }

    #[test]
    fn decision_requires_model_proposal_when_missing() {
        let package = package("ready_to_draft");
        let context = context("ready");
        let brief = build_development_brief(&package, &context, None);

        let decision = build_implementation_decision(&brief);

        assert_eq!(decision.status, "needs_model_proposal");
        assert!(decision
            .blockers
            .iter()
            .any(|block| block.contains("Falta propuesta")));
    }

    #[test]
    fn decision_blocks_needs_review_proposal() {
        let package = package("ready_to_draft");
        let context = context("ready");
        let proposal = LocalModelProposal {
            status: "needs_review".to_string(),
            model_used: "qwen3:8b".to_string(),
            summary: "Cambiar fuera de contexto.".to_string(),
            files_to_change: vec!["outside.py".to_string()],
            implementation_notes: vec!["Editar archivo.".to_string()],
            risks: vec!["Ruta propuesta fuera del contexto gobernado.".to_string()],
            gates: vec!["check:size".to_string()],
            raw_response: "{}".to_string(),
        };
        let brief = build_development_brief(&package, &context, Some(proposal));

        let decision = build_implementation_decision(&brief);

        assert_eq!(decision.status, "blocked");
        assert!(decision.warnings.iter().any(|warning| {
            warning.contains("fuera del contexto") || warning.contains("revision humana")
        }));
    }

    fn package(status: &str) -> DevelopmentWorkPackage {
        DevelopmentWorkPackage {
            repo_path: "C:\\OneEpis".to_string(),
            title: "Paquete".to_string(),
            objective: "Reducir archivo clinico near-limit".to_string(),
            status: status.to_string(),
            summary: "Listo".to_string(),
            branch_strategy: "agent/reducir".to_string(),
            files_to_inspect: vec!["AGENTS.md".to_string()],
            implementation_steps: vec!["Leer gobernanza.".to_string()],
            test_plan: vec![WorkPackageTest {
                gate: "check:size".to_string(),
                command: "npm run check:size".to_string(),
                purpose: "Validar tamano".to_string(),
                required: true,
            }],
            acceptance_criteria: vec!["No escribir sin PatchDraft.".to_string()],
            stop_conditions: vec!["Worktree sucio.".to_string()],
            gates: vec!["check:size".to_string()],
            warnings: if status == "blocked" {
                vec!["Resolver readiness.".to_string()]
            } else {
                Vec::new()
            },
            can_draft: status == "ready_to_draft",
            can_apply: false,
        }
    }

    fn context(status: &str) -> DevelopmentContextPack {
        DevelopmentContextPack {
            repo_path: "C:\\OneEpis".to_string(),
            objective: "Reducir archivo clinico near-limit".to_string(),
            status: status.to_string(),
            summary: "Contexto".to_string(),
            files: vec![ContextPackFile {
                path: "AGENTS.md".to_string(),
                kind: "file".to_string(),
                bytes: 20,
                lines: 2,
                sha256: "abc".to_string(),
                summary: "Guia".to_string(),
                excerpt: "No datos reales.".to_string(),
            }],
            warnings: if status == "partial" || status == "blocked" {
                vec!["Contexto parcial.".to_string()]
            } else {
                Vec::new()
            },
            prompt_notes: Vec::new(),
            gates: vec!["check:size".to_string()],
            total_bytes: 20,
            max_bytes: 1024,
        }
    }
}
