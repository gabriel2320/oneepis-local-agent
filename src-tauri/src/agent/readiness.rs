use crate::agent::ollama::get_ollama_status;
use crate::agent::repo::inspect_repository;
use crate::agent::types::{
    DevelopmentReadiness, OllamaStatus, ReadinessCheck, RepoInspection, SuggestedMicrocycle,
};
use std::collections::BTreeSet;

const ONEEPIS_CORE_GATES: &[&str] = &["check:api", "check:web", "check:contract", "check:size"];

pub async fn development_readiness(
    repo_path: &str,
    base_url: Option<String>,
) -> Result<DevelopmentReadiness, String> {
    let inspection = inspect_repository(repo_path)?;
    let ollama = get_ollama_status(base_url).await?;
    Ok(build_development_readiness(&inspection, &ollama))
}

pub fn build_development_readiness(
    inspection: &RepoInspection,
    ollama: &OllamaStatus,
) -> DevelopmentReadiness {
    let mut checks = Vec::new();
    let mut blockers = Vec::new();
    let mut warnings = Vec::new();

    push_check(
        &mut checks,
        "repo-git",
        inspection.is_git_repo,
        "Repo Git disponible.",
        "Elegir una carpeta que sea repositorio Git.",
        &mut blockers,
    );
    push_check(
        &mut checks,
        "worktree-clean",
        !inspection.dirty,
        "Worktree limpio para planificar y preparar apply futuro.",
        "Resolver o guardar cambios pendientes antes de apply.",
        &mut blockers,
    );
    push_check(
        &mut checks,
        "oneepis-profile",
        inspection.is_one_epis,
        "Adaptador OneEpis activo.",
        "Confirmar AGENTS.md, docs/GOVERNANCE.md y gates oficiales.",
        &mut warnings,
    );
    push_check(
        &mut checks,
        "ollama-local",
        ollama.available,
        "Ollama local responde.",
        "Iniciar Ollama o usar local_rules para plan conservador.",
        &mut warnings,
    );
    push_check(
        &mut checks,
        "policy-models",
        ollama.available && ollama.missing_policy_models.is_empty(),
        "Modelos de politica disponibles.",
        "Instalar modelos faltantes o ajustar AGENT_*_MODEL a modelos locales existentes.",
        &mut warnings,
    );

    for gate in ONEEPIS_CORE_GATES {
        push_check(
            &mut checks,
            &format!("gate:{gate}"),
            inspection.declared_gates.iter().any(|declared| declared == gate),
            &format!("{gate} declarado."),
            &format!("Agregar o mapear el gate {gate} en package.json antes de depender de esa superficie."),
            &mut warnings,
        );
    }

    let status = if !blockers.is_empty() {
        "blocked"
    } else if !warnings.is_empty() {
        "attention"
    } else {
        "ready"
    };
    let profile = if inspection.is_one_epis {
        "oneepis"
    } else {
        "generic"
    };

    let required_gates = required_gates(inspection);
    let suggested_microcycles = suggested_microcycles(inspection, &required_gates);
    let next_actions = next_actions(status, &blockers, &warnings, &required_gates);
    let summary = readiness_summary(status, inspection, ollama);

    DevelopmentReadiness {
        repo_path: inspection.repo_path.clone(),
        profile: profile.to_string(),
        status: status.to_string(),
        summary,
        checks,
        blockers,
        warnings,
        next_actions,
        suggested_microcycles,
        required_gates,
        local_model_summary: local_model_summary(ollama),
    }
}

fn push_check(
    checks: &mut Vec<ReadinessCheck>,
    name: &str,
    ok: bool,
    ready_detail: &str,
    action: &str,
    bucket: &mut Vec<String>,
) {
    checks.push(ReadinessCheck {
        name: name.to_string(),
        status: if ok { "ready" } else { "blocked" }.to_string(),
        detail: if ok {
            ready_detail.to_string()
        } else {
            action.to_string()
        },
        action: action.to_string(),
    });
    if !ok {
        bucket.push(action.to_string());
    }
}

fn required_gates(inspection: &RepoInspection) -> Vec<String> {
    let declared: BTreeSet<&str> = inspection
        .declared_gates
        .iter()
        .map(String::as_str)
        .collect();
    ONEEPIS_CORE_GATES
        .iter()
        .filter(|gate| declared.contains(**gate))
        .map(|gate| (*gate).to_string())
        .collect()
}

fn suggested_microcycles(
    inspection: &RepoInspection,
    required_gates: &[String],
) -> Vec<SuggestedMicrocycle> {
    if !inspection.is_one_epis {
        return vec![SuggestedMicrocycle {
            title: "Auditoria conservadora de repo generico".to_string(),
            objective: "Inspeccionar repo generico y proponer el menor microciclo verificable."
                .to_string(),
            risk_level: "yellow".to_string(),
            gates: inspection
                .declared_gates
                .first()
                .cloned()
                .into_iter()
                .collect(),
            reason: "Sin perfil OneEpis completo, el agente baja autonomia y solo planifica."
                .to_string(),
        }];
    }

    let mut suggestions = Vec::new();
    suggestions.push(SuggestedMicrocycle {
        title: "Dieta de archivo clinico near-limit".to_string(),
        objective:
            "Reducir un archivo clinico near-limit sin cambiar comportamiento y validar con check:size."
                .to_string(),
        risk_level: "green".to_string(),
        gates: gate_subset(required_gates, &["check:size"]),
        reason: "Mejora mantenibilidad y responde al guard de tamano antes de agregar comportamiento."
            .to_string(),
    });
    suggestions.push(SuggestedMicrocycle {
        title: "Contrato API minimo".to_string(),
        objective: "Auditar un endpoint paciente/ficha y agregar aprendizaje ejecutable en test o contrato."
            .to_string(),
        risk_level: "green".to_string(),
        gates: gate_subset(required_gates, &["check:api", "check:contract"]),
        reason: "Sigue la escalera paciente -> API -> contrato con gates oficiales.".to_string(),
    });
    suggestions.push(SuggestedMicrocycle {
        title: "Ruta web documentada".to_string(),
        objective:
            "Ajustar una pantalla existente de solo lectura y validar SCREEN_TREE/capabilities."
                .to_string(),
        risk_level: "yellow".to_string(),
        gates: gate_subset(required_gates, &["check:web", "check:size"]),
        reason: "Permite UI pequena sin abrir pantallas clinicas nuevas.".to_string(),
    });
    suggestions
}

fn gate_subset(required_gates: &[String], preferred: &[&str]) -> Vec<String> {
    preferred
        .iter()
        .filter(|gate| required_gates.iter().any(|candidate| candidate == **gate))
        .map(|gate| (*gate).to_string())
        .collect()
}

fn next_actions(
    status: &str,
    blockers: &[String],
    warnings: &[String],
    required_gates: &[String],
) -> Vec<String> {
    if status == "blocked" {
        return blockers
            .iter()
            .take(3)
            .cloned()
            .chain(["Volver a ejecutar readiness despues de resolver el bloqueo.".to_string()])
            .collect();
    }
    if !warnings.is_empty() {
        return warnings
            .iter()
            .take(2)
            .cloned()
            .chain(["Generar PatchDraft solo para un microciclo verde o amarillo.".to_string()])
            .collect();
    }
    let gate = required_gates
        .first()
        .cloned()
        .unwrap_or_else(|| "check:size".to_string());
    vec![
        "Generar PatchDraft revisable para el microciclo recomendado.".to_string(),
        format!("Ejecutar {gate} antes de decidir apply."),
        "Registrar resultado y detener el ciclo.".to_string(),
    ]
}

fn readiness_summary(status: &str, inspection: &RepoInspection, ollama: &OllamaStatus) -> String {
    let base = match status {
        "ready" => format!(
            "{} esta listo para microciclos OneEpis con modelos locales.",
            inspection.project_name
        ),
        "attention" => format!(
            "{} puede planificar, pero requiere atencion antes de operar con maxima utilidad local.",
            inspection.project_name
        ),
        _ => format!(
            "{} no esta listo para apply: hay bloqueos de repo o seguridad.",
            inspection.project_name
        ),
    };
    let model_note = if ollama.available {
        "Ollama responde."
    } else {
        "Ollama no responde; se usaran reglas locales."
    };
    format!("{base} {model_note}")
}

fn local_model_summary(ollama: &OllamaStatus) -> String {
    if !ollama.available {
        return "Ollama no disponible; planificacion limitada a local_rules.".to_string();
    }
    if ollama.missing_policy_models.is_empty() {
        return format!(
            "{} modelos locales disponibles; politica completa.",
            ollama.models.len()
        );
    }
    format!(
        "{} modelos locales disponibles; faltan: {}.",
        ollama.models.len(),
        ollama.missing_policy_models.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::{GovernanceDocument, ModelPolicy, OllamaModel};

    #[test]
    fn clean_oneepis_with_models_is_ready() {
        let inspection = inspection(false);
        let ollama = ollama(true, Vec::new());

        let readiness = build_development_readiness(&inspection, &ollama);

        assert_eq!(readiness.status, "ready");
        assert!(readiness.blockers.is_empty());
        assert!(readiness
            .suggested_microcycles
            .iter()
            .any(|item| item.objective.contains("check:size")));
        assert!(readiness.required_gates.contains(&"check:api".to_string()));
    }

    #[test]
    fn dirty_oneepis_blocks_apply_but_suggests_repair() {
        let inspection = inspection(true);
        let ollama = ollama(true, Vec::new());

        let readiness = build_development_readiness(&inspection, &ollama);

        assert_eq!(readiness.status, "blocked");
        assert!(readiness
            .next_actions
            .iter()
            .any(|action| action.contains("cambios pendientes")));
    }

    #[test]
    fn missing_models_are_attention_not_blocker() {
        let inspection = inspection(false);
        let ollama = ollama(true, vec!["qwen3:8b".to_string()]);

        let readiness = build_development_readiness(&inspection, &ollama);

        assert_eq!(readiness.status, "attention");
        assert!(readiness.blockers.is_empty());
        assert!(readiness
            .warnings
            .iter()
            .any(|warning| warning.contains("modelos")));
    }

    fn inspection(dirty: bool) -> RepoInspection {
        RepoInspection {
            repo_path: "C:\\OneEpis".to_string(),
            project_name: "OneEpis".to_string(),
            is_git_repo: true,
            is_one_epis: true,
            current_branch: "codex/test".to_string(),
            dirty,
            status_text: if dirty {
                " M README.md"
            } else {
                "## codex/test"
            }
            .to_string(),
            governance_documents: vec![GovernanceDocument {
                path: "docs/GOVERNANCE.md".to_string(),
                title: "Gobernanza".to_string(),
                sha256: "test".to_string(),
                bytes: 1,
                present: true,
            }],
            declared_gates: vec![
                "check:api".to_string(),
                "check:web".to_string(),
                "check:contract".to_string(),
                "check:size".to_string(),
            ],
            detected_rules: Vec::new(),
            blocks: Vec::new(),
        }
    }

    fn ollama(available: bool, missing_policy_models: Vec<String>) -> OllamaStatus {
        OllamaStatus {
            base_url: "http://localhost:11434".to_string(),
            available,
            message: "test".to_string(),
            models: vec![OllamaModel {
                name: "qwen3:8b".to_string(),
                size: 1,
                family: "qwen".to_string(),
                parameters: "8B".to_string(),
                quantization: "Q4".to_string(),
            }],
            policy: ModelPolicy::default(),
            missing_policy_models,
        }
    }
}
