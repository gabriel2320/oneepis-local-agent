use crate::agent::readiness::development_readiness;
use crate::agent::repo::inspect_repository;
use crate::agent::safety::sanitize_log;
use crate::agent::types::{
    DevelopmentReadiness, DevelopmentWorkPackage, RepoInspection, WorkPackageTest,
};

pub async fn development_work_package(
    repo_path: &str,
    objective: &str,
    base_url: Option<String>,
) -> Result<DevelopmentWorkPackage, String> {
    let inspection = inspect_repository(repo_path)?;
    let readiness = development_readiness(repo_path, base_url).await?;
    Ok(build_development_work_package(
        &inspection,
        &readiness,
        objective,
    ))
}

pub fn build_development_work_package(
    inspection: &RepoInspection,
    readiness: &DevelopmentReadiness,
    objective: &str,
) -> DevelopmentWorkPackage {
    let objective = sanitize_log(objective);
    let gates = select_gates(&objective, readiness);
    let status = if readiness.status == "blocked" {
        "blocked"
    } else if gates.is_empty() {
        "needs_gate"
    } else {
        "ready_to_draft"
    };
    let mut warnings = readiness.warnings.clone();
    warnings.extend(readiness.blockers.clone());
    if !inspection.is_one_epis {
        warnings.push("Repo sin perfil OneEpis completo; mantener plan conservador.".to_string());
    }

    DevelopmentWorkPackage {
        repo_path: inspection.repo_path.clone(),
        title: title_for(&objective),
        objective: objective.clone(),
        status: status.to_string(),
        summary: summary_for(status, inspection, &gates),
        branch_strategy: format!("agent/{}", slug(&objective)),
        files_to_inspect: files_to_inspect(&objective, inspection),
        implementation_steps: implementation_steps(&objective, status),
        test_plan: test_plan(&gates),
        acceptance_criteria: acceptance_criteria(&objective, &gates),
        stop_conditions: stop_conditions(),
        gates,
        warnings,
        can_draft: status == "ready_to_draft",
        can_apply: false,
    }
}

fn select_gates(objective: &str, readiness: &DevelopmentReadiness) -> Vec<String> {
    let text = objective.to_lowercase();
    let mut gates = Vec::new();
    for (needle, gate) in [
        ("api", "check:api"),
        ("endpoint", "check:api"),
        ("postgres", "check:api"),
        ("contrato", "check:contract"),
        ("openapi", "check:contract"),
        ("web", "check:web"),
        ("pantalla", "check:web"),
        ("ruta", "check:web"),
        ("screen", "check:web"),
        ("size", "check:size"),
        ("tamano", "check:size"),
        ("near-limit", "check:size"),
        ("archivo", "check:size"),
    ] {
        if text.contains(needle) && readiness.required_gates.iter().any(|item| item == gate) {
            push_unique(&mut gates, gate);
        }
    }
    if gates.is_empty() {
        for gate in ["check:size", "check:api"] {
            if readiness.required_gates.iter().any(|item| item == gate) {
                push_unique(&mut gates, gate);
            }
        }
    }
    gates
}

fn files_to_inspect(objective: &str, inspection: &RepoInspection) -> Vec<String> {
    let text = objective.to_lowercase();
    let mut files = vec![
        "AGENTS.md".to_string(),
        "docs/GOVERNANCE.md".to_string(),
        "docs/CODEX_PLAN.md".to_string(),
    ];
    if text.contains("api") || text.contains("endpoint") || text.contains("postgres") {
        files.extend([
            "apps/api/src/oneepis_api/api/v1/routes".to_string(),
            "apps/api/src/oneepis_api/services".to_string(),
            "apps/api/tests".to_string(),
        ]);
    }
    if text.contains("web") || text.contains("pantalla") || text.contains("ruta") {
        files.extend([
            "apps/web/src".to_string(),
            "docs/SCREEN_TREE.md".to_string(),
            "apps/web/src/lib/type-contracts".to_string(),
        ]);
    }
    if text.contains("contrato") || text.contains("openapi") {
        files.extend([
            "docs".to_string(),
            "packages".to_string(),
            "apps/api".to_string(),
        ]);
    }
    if text.contains("size") || text.contains("tamano") || text.contains("near-limit") {
        files.extend([
            "scripts/check-file-size.mjs".to_string(),
            "apps/api/src/oneepis_api/services/clinical_intent.py".to_string(),
            "apps/web/src/components/clinical".to_string(),
        ]);
    }
    if !inspection.is_one_epis {
        files.push("package.json".to_string());
    }
    dedupe(files)
}

fn implementation_steps(objective: &str, status: &str) -> Vec<String> {
    if status == "blocked" {
        return vec![
            "Resolver el bloqueo de readiness antes de editar.".to_string(),
            "Volver a generar DevelopmentWorkPackage.".to_string(),
            "Crear PatchDraft solo si el estado cambia a ready_to_draft.".to_string(),
        ];
    }
    vec![
        "Leer gobernanza y archivos indicados antes de tocar codigo.".to_string(),
        format!("Reducir objetivo a una sola decision: {objective}"),
        "Hacer el cambio minimo sin crear superficies clinicas nuevas.".to_string(),
        "Agregar aprendizaje ejecutable: test, gate, contrato o tipo.".to_string(),
        "Generar PatchDraft y revisar diff antes de aplicar.".to_string(),
    ]
}

fn test_plan(gates: &[String]) -> Vec<WorkPackageTest> {
    gates
        .iter()
        .map(|gate| WorkPackageTest {
            gate: gate.clone(),
            command: format!("npm run {gate}"),
            purpose: purpose_for_gate(gate),
            required: true,
        })
        .collect()
}

fn acceptance_criteria(objective: &str, gates: &[String]) -> Vec<String> {
    let mut criteria = vec![
        "El cambio queda en un PatchDraft revisable antes de escritura real.".to_string(),
        "No se agregan secretos, PHI ni identificadores reales.".to_string(),
        "No hay push automatico ni comandos fuera de acciones tipadas.".to_string(),
        format!("El objetivo queda acotado y verificable: {objective}"),
    ];
    for gate in gates {
        criteria.push(format!(
            "{gate} pasa o el ciclo se detiene con salida registrada."
        ));
    }
    criteria
}

fn stop_conditions() -> Vec<String> {
    vec![
        "Worktree sucio en el repo objetivo.".to_string(),
        "Riesgo rojo o alcance clinico sin contrato explicito.".to_string(),
        "Gate requerido no declarado o fallido.".to_string(),
        "Diff toca rutas fuera del repo o mas de 8 archivos.".to_string(),
        "Falta confirmacion humana para apply.".to_string(),
    ]
}

fn title_for(objective: &str) -> String {
    let clean = objective.trim();
    if clean.is_empty() {
        "Paquete de trabajo OneEpis".to_string()
    } else {
        format!("Paquete: {}", clean.chars().take(72).collect::<String>())
    }
}

fn summary_for(status: &str, inspection: &RepoInspection, gates: &[String]) -> String {
    match status {
        "blocked" => format!(
            "{} requiere resolver readiness antes de programar.",
            inspection.project_name
        ),
        "needs_gate" => format!(
            "{} necesita un gate declarado antes de crear PatchDraft.",
            inspection.project_name
        ),
        _ => format!(
            "{} puede preparar PatchDraft con gates: {}.",
            inspection.project_name,
            gates.join(", ")
        ),
    }
}

fn purpose_for_gate(gate: &str) -> String {
    match gate {
        "check:api" => {
            "Validar API, servicios, modelos y pruebas backend relacionadas.".to_string()
        }
        "check:web" => "Validar UI, rutas visibles y contratos frontend.".to_string(),
        "check:contract" => "Validar OpenAPI/contratos antes de acoplar superficies.".to_string(),
        "check:size" => {
            "Evitar crecer archivos near-limit y confirmar SCREEN_TREE/capabilities.".to_string()
        }
        _ => "Validar el gate declarado por el repo objetivo.".to_string(),
    }
}

fn slug(input: &str) -> String {
    let mut slug = String::new();
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if matches!(ch, ' ' | '-' | '_' | '/' | '\\') && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "oneepis-work".to_string()
    } else {
        slug.chars().take(48).collect()
    }
}

fn push_unique(items: &mut Vec<String>, item: &str) {
    if !items.iter().any(|candidate| candidate == item) {
        items.push(item.to_string());
    }
}

fn dedupe(items: Vec<String>) -> Vec<String> {
    let mut deduped = Vec::new();
    for item in items {
        push_unique(&mut deduped, &item);
    }
    deduped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::{DevelopmentReadiness, ReadinessCheck, SuggestedMicrocycle};

    #[test]
    fn api_objective_builds_testable_package() {
        let inspection = inspection(false);
        let readiness = readiness("ready");

        let package = build_development_work_package(
            &inspection,
            &readiness,
            "Auditar endpoint API paciente y contrato OpenAPI",
        );

        assert_eq!(package.status, "ready_to_draft");
        assert!(package.can_draft);
        assert!(!package.can_apply);
        assert!(package.gates.contains(&"check:api".to_string()));
        assert!(package.gates.contains(&"check:contract".to_string()));
        assert!(package
            .files_to_inspect
            .contains(&"apps/api/src/oneepis_api/api/v1/routes".to_string()));
    }

    #[test]
    fn blocked_readiness_blocks_package_apply_path() {
        let inspection = inspection(true);
        let readiness = readiness("blocked");

        let package = build_development_work_package(
            &inspection,
            &readiness,
            "Reducir archivo clinico near-limit",
        );

        assert_eq!(package.status, "blocked");
        assert!(!package.can_draft);
        assert!(package
            .implementation_steps
            .iter()
            .any(|step| step.contains("Resolver el bloqueo")));
    }

    fn inspection(dirty: bool) -> RepoInspection {
        RepoInspection {
            repo_path: "C:\\OneEpis".to_string(),
            project_name: "OneEpis".to_string(),
            is_git_repo: true,
            is_one_epis: true,
            current_branch: "codex/test".to_string(),
            dirty,
            status_text: String::new(),
            governance_documents: Vec::new(),
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

    fn readiness(status: &str) -> DevelopmentReadiness {
        DevelopmentReadiness {
            repo_path: "C:\\OneEpis".to_string(),
            profile: "oneepis".to_string(),
            status: status.to_string(),
            summary: String::new(),
            checks: vec![ReadinessCheck {
                name: "worktree-clean".to_string(),
                status: status.to_string(),
                detail: String::new(),
                action: String::new(),
            }],
            blockers: if status == "blocked" {
                vec!["Resolver o guardar cambios pendientes antes de apply.".to_string()]
            } else {
                Vec::new()
            },
            warnings: Vec::new(),
            next_actions: Vec::new(),
            suggested_microcycles: vec![SuggestedMicrocycle {
                title: "Dieta".to_string(),
                objective: "Reducir archivo clinico near-limit".to_string(),
                risk_level: "green".to_string(),
                gates: vec!["check:size".to_string()],
                reason: String::new(),
            }],
            required_gates: vec![
                "check:api".to_string(),
                "check:web".to_string(),
                "check:contract".to_string(),
                "check:size".to_string(),
            ],
            local_model_summary: String::new(),
        }
    }
}
