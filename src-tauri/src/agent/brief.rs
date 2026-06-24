use crate::agent::context_pack::build_development_context_pack;
use crate::agent::ollama::ask_for_development_proposal;
use crate::agent::repo::inspect_repository;
use crate::agent::safety::sanitize_log;
use crate::agent::types::{
    DevelopmentBrief, DevelopmentContextPack, DevelopmentWorkPackage, LocalModelProposal,
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
