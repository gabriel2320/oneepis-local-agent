use crate::agent::types::{MicroPlan, RepoInspection};

pub fn apply_oneepis_governance(plan: &mut MicroPlan, inspection: &RepoInspection) {
    push_unique(
        &mut plan.warnings,
        "OneEpis: aplicar escalera paciente -> ficha -> papel -> API -> PostgreSQL -> auditoria -> permisos -> OpenAPI.",
    );
    push_unique(
        &mut plan.warnings,
        "OneEpis: no crear documentos nuevos si README, CURRENT_STATE, GOVERNANCE, SCREEN_TREE o CODEX_PLAN pueden absorber la decision.",
    );
    push_unique(
        &mut plan.warnings,
        "OneEpis: dejar aprendizaje ejecutable mediante test, gate, contrato o tipo.",
    );

    let objective = normalized_objective(&plan.objective);
    let mut hard_blocks = Vec::new();
    for (needle, reason) in [
        (
            "dashboard central",
            "Dashboard central nuevo esta bloqueado por gobernanza.",
        ),
        (
            "chat libre",
            "Chat libre generico esta bloqueado por gobernanza.",
        ),
        (
            "rag amplio",
            "RAG documental amplio esta bloqueado por gobernanza.",
        ),
        (
            "ia externa",
            "IA externa identificada es cambio rojo y requiere contrato previo.",
        ),
        (
            "receta valida",
            "Receta valida requiere firma/folio/contrato clinico previo.",
        ),
        (
            "firma clinica",
            "Firma clinica real requiere contrato legal/clinico previo.",
        ),
        (
            "orden ejecutable",
            "Orden ejecutable esta bloqueada sin contrato clinico previo.",
        ),
        (
            "indicacion ejecutable",
            "Indicacion ejecutable esta bloqueada sin contrato clinico previo.",
        ),
        (
            "agenda productiva",
            "Agenda productiva sigue fuera del trabajo inmediato permitido.",
        ),
        (
            "importador pdf",
            "Importador PDF completo esta bloqueado por limites activos.",
        ),
        (
            "datos sensibles",
            "Datos sensibles reales no pueden entrar al repo ni a la bitacora.",
        ),
        (
            "phi real",
            "PHI real no puede entrar al repo ni a la bitacora.",
        ),
        (
            "arquitectura nueva",
            "Arquitectura nueva es cambio rojo y requiere contrato/ADR previo.",
        ),
    ] {
        if objective.contains(needle) {
            hard_blocks.push(reason.to_string());
        }
    }

    for surface in oneepis_surfaces(&objective) {
        push_unique(&mut plan.touched_surfaces, surface);
    }
    for gate in oneepis_required_gates(&objective) {
        if inspection
            .declared_gates
            .iter()
            .any(|declared| declared == gate)
        {
            push_unique(&mut plan.required_gates, gate);
        }
    }

    if objective.contains("pantalla")
        || objective.contains("ruta")
        || objective.contains("apps/web/src/app")
    {
        push_unique(
            &mut plan.warnings,
            "OneEpis: toda ruta visible debe actualizar SCREEN_TREE y Screen Capability Registry.",
        );
    }
    if objective.contains("dependencia")
        || objective.contains("npm install")
        || objective.contains("paquete nuevo")
    {
        push_unique(
            &mut plan.warnings,
            "OneEpis: dependencia nueva requiere justificar complejidad, alternativas, mantenimiento y seguridad.",
        );
        escalate_yellow(plan);
    }
    if objective.contains("documento nuevo") || objective.contains("crear documento") {
        push_unique(
            &mut plan.warnings,
            "OneEpis: evitar documento nuevo; actualizar una fuente canonica existente.",
        );
        escalate_yellow(plan);
    }

    if !hard_blocks.is_empty() {
        plan.risk_level = "red".to_string();
        plan.blocked = true;
        plan.steps = vec![
            "Detener el alcance rojo antes de implementar.".to_string(),
            "Redactar contrato clinico/tecnico explicito en la fuente canonica correspondiente."
                .to_string(),
            "Elegir un microciclo permitido dentro de paciente/ficha/papel/API/PostgreSQL/auditoria/permisos/OpenAPI."
                .to_string(),
        ];
        for reason in hard_blocks {
            push_unique(&mut plan.warnings, &reason);
        }
    } else if plan.risk_level == "red" {
        plan.blocked = true;
    } else if plan.required_gates.is_empty() {
        push_unique(
            &mut plan.warnings,
            "OneEpis: no hay gate oficial declarado para esta superficie; reducir alcance.",
        );
        escalate_yellow(plan);
    }

    if !plan.required_gates.is_empty() {
        plan.recommended_gate = plan.required_gates[0].clone();
    }
}

fn normalized_objective(objective: &str) -> String {
    objective
        .to_lowercase()
        .replace('á', "a")
        .replace('é', "e")
        .replace('í', "i")
        .replace('ó', "o")
        .replace('ú', "u")
}

fn oneepis_surfaces(objective: &str) -> Vec<&'static str> {
    let mut surfaces = Vec::new();
    if contains_any(
        objective,
        &[
            "api",
            "backend",
            "fastapi",
            "endpoint",
            "postgres",
            "alembic",
            "migracion",
        ],
    ) {
        surfaces.push("API/PostgreSQL");
    }
    if contains_any(
        objective,
        &[
            "web",
            "ui",
            "frontend",
            "pantalla",
            "ruta",
            "componente",
            "mostrar",
        ],
    ) {
        surfaces.push("web/ficha");
    }
    if contains_any(objective, &["openapi", "contrato", "contract"]) {
        surfaces.push("OpenAPI/contratos");
    }
    if contains_any(
        objective,
        &["papel", "print", "impresion", "documento clinico"],
    ) {
        surfaces.push("papel clinico");
    }
    if contains_any(
        objective,
        &[
            "paciente",
            "ficha",
            "antecedente",
            "evento",
            "laboratorio",
            "riesgo",
        ],
    ) {
        surfaces.push("paciente/ficha");
    }
    if surfaces.is_empty() {
        surfaces.push("paciente/ficha");
    }
    surfaces
}

fn oneepis_required_gates(objective: &str) -> Vec<&'static str> {
    let mut gates = Vec::new();
    if contains_any(
        objective,
        &[
            "api",
            "backend",
            "fastapi",
            "endpoint",
            "postgres",
            "alembic",
            "migracion",
            "permisos",
            "auditoria",
        ],
    ) {
        gates.push("check:api");
    }
    if contains_any(objective, &["openapi", "contrato", "contract"]) {
        gates.push("check:contract");
    }
    if contains_any(
        objective,
        &[
            "web",
            "ui",
            "frontend",
            "pantalla",
            "ruta",
            "componente",
            "mostrar",
        ],
    ) {
        gates.push("check:web");
    }
    if contains_any(
        objective,
        &[
            "e2e",
            "papel",
            "print",
            "impresion",
            "documento clinico",
            "ruta visible",
        ],
    ) {
        gates.push("check:e2e");
    }
    if contains_any(
        objective,
        &["size", "tamano", "extraer", "archivo grande", "near-limit"],
    ) {
        gates.push("check:size");
    }
    if gates.is_empty() {
        gates.push("check:size");
    }
    gates
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn escalate_yellow(plan: &mut MicroPlan) {
    if plan.risk_level != "red" {
        plan.risk_level = "yellow".to_string();
    }
}

fn push_unique(items: &mut Vec<String>, item: &str) {
    if !items.iter().any(|candidate| candidate == item) {
        items.push(item.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::{GovernanceDocument, RepoInspection};

    #[test]
    fn allowed_patient_work_is_not_blocked() {
        let inspection = oneepis_inspection();
        let mut plan = base_plan("Mostrar antecedentes de solo lectura en ficha paciente");

        apply_oneepis_governance(&mut plan, &inspection);

        assert!(!plan.blocked);
        assert_ne!(plan.risk_level, "red");
        assert!(plan.required_gates.contains(&"check:web".to_string()));
        assert!(plan
            .touched_surfaces
            .contains(&"paciente/ficha".to_string()));
    }

    #[test]
    fn blocked_scope_becomes_red_without_implementation_steps() {
        let inspection = oneepis_inspection();
        let mut plan = base_plan("Crear dashboard central con RAG amplio y receta valida");

        apply_oneepis_governance(&mut plan, &inspection);

        assert!(plan.blocked);
        assert_eq!(plan.risk_level, "red");
        assert!(plan
            .warnings
            .iter()
            .any(|warning| warning.contains("Dashboard central")));
        assert!(plan
            .steps
            .iter()
            .any(|step| step.contains("Detener el alcance rojo")));
        assert!(!plan
            .steps
            .iter()
            .any(|step| step.starts_with("Implementar dashboard")));
    }

    fn base_plan(objective: &str) -> MicroPlan {
        MicroPlan {
            objective: objective.to_string(),
            recommended_gate: String::new(),
            risk_level: String::new(),
            touched_surfaces: Vec::new(),
            required_gates: Vec::new(),
            steps: vec!["Reducir a microciclo verificable.".to_string()],
            warnings: Vec::new(),
            blocked: false,
            model_used: "test".to_string(),
        }
    }

    fn oneepis_inspection() -> RepoInspection {
        RepoInspection {
            repo_path: "C:\\OneEpis".to_string(),
            project_name: "OneEpis".to_string(),
            is_git_repo: true,
            is_one_epis: true,
            current_branch: "main".to_string(),
            dirty: false,
            status_text: "## main".to_string(),
            governance_documents: vec![GovernanceDocument {
                path: "docs/GOVERNANCE.md".to_string(),
                title: "Gobierno Tecnico".to_string(),
                sha256: "test".to_string(),
                bytes: 1,
                present: true,
            }],
            declared_gates: vec![
                "check".to_string(),
                "check:api".to_string(),
                "check:contract".to_string(),
                "check:e2e".to_string(),
                "check:screens".to_string(),
                "check:size".to_string(),
                "check:web".to_string(),
            ],
            detected_rules: Vec::new(),
            blocks: Vec::new(),
        }
    }
}
