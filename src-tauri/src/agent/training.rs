use crate::agent::repo::{canonical_repo, git, inspect_repository};
use crate::agent::safety::sha256_hex;
use crate::agent::types::{TrainingPlan, TrainingRequest, TrainingRun, TrainingScenario};

use chrono::Utc;
use std::path::Path;

const MAX_TRAINING_CYCLES: u8 = 3;

const TRAINING_RULES: &[&str] = &[
    "Una tarea por rama agent/train-*.",
    "Maximo 3 ciclos concatenados.",
    "No hay push automatico.",
    "Si necesita tabla, endpoint, permiso nuevo o ruta nueva, detenerse y crear plan.",
    "Si toca escritura clinica: API, PostgreSQL, permisos, auditoria, OpenAPI y tests son obligatorios.",
    "Si toca IA: siempre opcional, lateral, sin diagnostico ni escritura automatica.",
    "Usar solo IA local: Ollama y reglas locales; sin proveedores externos.",
];

pub fn list_training_scenarios() -> Vec<TrainingScenario> {
    training_scenarios()
}

pub fn training_plan(request: TrainingRequest) -> Result<TrainingPlan, String> {
    let scenario = training_scenario(&request.scenario_id)?;
    let inspection = inspect_repository(&request.repo_path)?;
    let mut blockers = common_training_blocks(&inspection, &scenario, request.cycles);
    let mut warnings = Vec::new();

    if scenario.execution_mode == "plan_only" {
        warnings.push(
            "Este escenario debe terminar en plan/contrato minimo; no debe aplicar UI amplia ni cambios clinicos."
                .to_string(),
        );
    }
    if !scenario.manual_gates.is_empty() {
        warnings.push(format!(
            "Requiere verificacion manual/local adicional: {}.",
            scenario.manual_gates.join(", ")
        ));
    }
    if inspection.dirty && inspection.current_branch == scenario.branch {
        warnings.push(
            "Hay cambios pendientes en la rama de entrenamiento; solo continuar si son del escenario."
                .to_string(),
        );
    } else if inspection.dirty {
        blockers.push(format!(
            "Hay cambios pendientes fuera de {}; guarda o descarta antes de preparar TRAIN.",
            scenario.branch
        ));
    }

    let status = if blockers.is_empty() {
        if scenario.execution_mode == "plan_only" {
            "plan_only_ready"
        } else {
            "ready"
        }
    } else {
        "blocked"
    };
    let next_actions = if blockers.is_empty() {
        vec![
            format!("Preparar rama local {}.", scenario.branch),
            "Crear brief con contexto y, si se usa IA, consultar solo Ollama.".to_string(),
            "Ejecutar como maximo 3 ciclos y detenerse si aparece una condicion de stop."
                .to_string(),
            "Correr gates declarados antes de cualquier commit local manual.".to_string(),
        ]
    } else {
        blockers
            .iter()
            .take(3)
            .cloned()
            .chain(["No iniciar entrenamiento hasta resolver bloqueos.".to_string()])
            .collect()
    };

    Ok(TrainingPlan {
        repo_path: inspection.repo_path,
        scenario,
        status: status.to_string(),
        cycles: request.cycles,
        max_cycles: MAX_TRAINING_CYCLES,
        blockers,
        warnings,
        next_actions,
        no_push: true,
        local_ai_only: true,
        summary: "Plan de entrenamiento local gobernado para OneEpis.".to_string(),
    })
}

pub async fn prepare_training_scenario(request: TrainingRequest) -> Result<TrainingRun, String> {
    let scenario = training_scenario(&request.scenario_id)?;
    let plan = training_plan(request.clone())?;
    if !plan.blockers.is_empty() {
        return Ok(blocked_run(
            &plan.repo_path,
            &scenario,
            request.cycles,
            plan.blockers,
        ));
    }
    let repo = canonical_repo(&request.repo_path)?;
    let branch = ensure_training_branch(&repo, &scenario.branch)?;
    Ok(TrainingRun {
        id: run_id(&plan.repo_path, &scenario.id),
        scenario_id: scenario.id,
        status: "ready_for_training".to_string(),
        repo_path: plan.repo_path,
        branch,
        cycles: request.cycles,
        blockers: Vec::new(),
        warnings: plan
            .warnings
            .into_iter()
            .chain(["Rama preparada; no se hicieron cambios ni push.".to_string()])
            .collect(),
        next_actions: vec![
            "Pedir brief local gobernado con el objetivo TRAIN seleccionado.".to_string(),
            "Usar solo Ollama si se consulta modelo; no usar IA externa.".to_string(),
            "Si aparece una condicion de stop, documentar plan y detenerse.".to_string(),
        ],
        no_push: true,
        local_ai_only: true,
        summary: "Escenario TRAIN preparado en rama local.".to_string(),
    })
}

fn training_scenarios() -> Vec<TrainingScenario> {
    vec![
        scenario(
            "TRAIN-001",
            "Reducir clinical_intent.py sin cambiar conducta",
            "Extraer otra familia de reglas deterministicas manteniendo respuestas identicas.",
            "agent/train-001-reducir-clinical-intent",
            &[
                "Refactor backend con tests.",
                "Contrato estable.",
                "No tocar prompts ni endpoints.",
            ],
            &["check:api", "check:contract", "check:size"],
            &[],
            &["clinical_intent.py", "clinical_intent_*"],
            "guided_refactor",
        ),
        scenario(
            "TRAIN-002",
            "Detectar drift entre OpenAPI y tipos frontend",
            "Auditar tipos manuales Assistant Read / Clinical Record contra openapi.json.",
            "agent/train-002-openapi-frontend-drift",
            &[
                "Distinguir drift real de ruido.",
                "Proponer patch minimo.",
            ],
            &["check:contract", "check:web"],
            &[],
            &["packages/contracts/openapi.json", "assistant-read", "clinical-record", "types"],
            "audit_then_patch",
        ),
        scenario(
            "TRAIN-003",
            "Mejorar una pantalla near-limit sin cambiar UX",
            "Tomar un archivo React sobre 300 lineas y extraer componentes de dominio.",
            "agent/train-003-react-near-limit",
            &[
                "Refactor UI puro.",
                "Preservar textos, rutas y permisos.",
            ],
            &["check:web", "check:e2e"],
            &[],
            &["apps/web/src/components/clinical"],
            "guided_refactor",
        ),
        scenario(
            "TRAIN-004",
            "Endurecer un flujo E2E fragil",
            "Encontrar selectores ambiguos en Playwright y hacerlos mas robustos.",
            "agent/train-004-e2e-fragil",
            &[
                "No agregar snapshots pesados.",
                "Usar roles, scopes y texto exacto.",
            ],
            &["check:e2e"],
            &[],
            &["apps/web/tests/e2e"],
            "test_refactor",
        ),
        scenario(
            "TRAIN-005",
            "Auditoria de permisos enfermeria/preconsulta",
            "Verificar que enfermeria solo pueda completar preconsulta minima y nada mas.",
            "agent/train-005-permisos-enfermeria",
            &[
                "Revisar backend, frontend y tests.",
                "No abrir permisos generales.",
            ],
            &["check:api", "check:web"],
            &[],
            &["permissions", "preconsult", "ambulatory", "nursing"],
            "audit_then_patch",
        ),
        scenario(
            "TRAIN-006",
            "Papel serio, sin documento nuevo",
            "Mejorar jerarquia visual de una ruta print existente.",
            "agent/train-006-papel-print",
            &[
                "CSS print.",
                "No habilitar receta, firma ni folio legal.",
            ],
            &["check:web", "check:e2e"],
            &[],
            &["print", "paper", "css"],
            "guided_refactor",
        ),
        scenario(
            "TRAIN-007",
            "Migracion limpia desde PostgreSQL vacio",
            "Crear o verificar una prueba de bootstrap Alembic desde base temporal limpia.",
            "agent/train-007-bootstrap-alembic",
            &[
                "Migraciones reales.",
                "No confiar solo en SQLite/tests API.",
            ],
            &["check:api"],
            &["alembic:bootstrap-postgresql-local"],
            &["alembic", "migrations", "postgresql"],
            "manual_gate_required",
        ),
        scenario(
            "TRAIN-008",
            "Simular fallo de Ollama sin romper ficha",
            "Probar que la UI y API siguen operativas si Ollama no responde.",
            "agent/train-008-ollama-fallback",
            &[
                "IA opcional.",
                "Fallback controlado.",
                "No bloquear guardado clinico.",
            ],
            &["check:web"],
            &["backend:mocked-ollama-tests"],
            &["ollama", "ai", "fallback"],
            "failure_drill",
        ),
        scenario(
            "TRAIN-009",
            "Corregir contaminacion demo",
            "Auditar fixtures/demo para nombres externos, datos realistas o residuos de otros proyectos.",
            "agent/train-009-contaminacion-demo",
            &[
                "Higiene clinica.",
                "Datos sinteticos obvios.",
                "No PHI.",
            ],
            &["check:web"],
            &["text-search:documented"],
            &["demo", "fixtures"],
            "audit_then_patch",
        ),
        scenario(
            "TRAIN-010",
            "Preparar contrato antes de UI amplia",
            "Tomar una idea futura y producir solo contrato/plan minimo.",
            "agent/train-010-contrato-antes-ui",
            &[
                "Detenerse antes de implementar si falta API/PostgreSQL/permisos/auditoria.",
                "Docs minimas sin documentos nuevos innecesarios.",
            ],
            &["check:size"],
            &["git:diff-check"],
            &["docs", "contracts"],
            "plan_only",
        ),
        scenario(
            "TRAIN-011",
            "Refactor de cliente API por dominio",
            "Separar funciones de un cliente frontend grande sin cambiar imports publicos.",
            "agent/train-011-cliente-api-dominio",
            &[
                "Compatibilidad temporal.",
                "Evitar barrels eternos.",
            ],
            &["check:web"],
            &[],
            &["apps/web/src/lib/api"],
            "guided_refactor",
        ),
        scenario(
            "TRAIN-012",
            "Revisar seguridad CI",
            "Confirmar que gitleaks bloquea y que el resto sigue report-only documentado.",
            "agent/train-012-seguridad-ci",
            &[
                "No endurecer senales ruidosas sin politica.",
                "Distinguir bloqueante de report-only.",
            ],
            &[],
            &["git:diff-check", "ci:remote-if-pr"],
            &[".github", "security", "gitleaks"],
            "audit_only",
        ),
        scenario(
            "TRAIN-013",
            "Performance con dataset sintetico",
            "Crear una prueba local controlada para ficha con muchos eventos ficticios.",
            "agent/train-013-performance-sintetica",
            &[
                "No commitear datos pesados.",
                "Medir antes de optimizar.",
            ],
            &[],
            &["script-temporal-or-bounded-test"],
            &["performance", "synthetic", "events"],
            "experiment_local",
        ),
        scenario(
            "TRAIN-014",
            "Revisar ausencia de fallback silencioso por ID",
            "Auditar rutas print o clinicas con [id] para confirmar que no muestran otro registro si falta el solicitado.",
            "agent/train-014-identidad-estricta-id",
            &[
                "Seguridad clinica por identidad estricta.",
                "No mostrar registros sustitutos.",
            ],
            &["check:e2e"],
            &[],
            &["print", "routes", "[id]", "not-found"],
            "test_refactor",
        ),
        scenario(
            "TRAIN-015",
            "Actualizar documentacion viva despues de merge real",
            "Detectar contradicciones entre CURRENT_STATE, GOVERNANCE, CODEX_PLAN y codigo.",
            "agent/train-015-documentacion-viva",
            &[
                "Docs minimas.",
                "No crear documentos nuevos.",
            ],
            &["check:size"],
            &["git:diff-check"],
            &["CURRENT_STATE", "GOVERNANCE", "CODEX_PLAN", "docs"],
            "docs_only",
        ),
    ]
}

fn scenario(
    id: &str,
    title: &str,
    objective: &str,
    branch: &str,
    teaches: &[&str],
    gates: &[&str],
    manual_gates: &[&str],
    allowed_surfaces: &[&str],
    execution_mode: &str,
) -> TrainingScenario {
    TrainingScenario {
        id: id.to_string(),
        title: title.to_string(),
        objective: objective.to_string(),
        branch: branch.to_string(),
        teaches: teaches.iter().map(|item| item.to_string()).collect(),
        gates: gates.iter().map(|item| item.to_string()).collect(),
        manual_gates: manual_gates.iter().map(|item| item.to_string()).collect(),
        allowed_surfaces: allowed_surfaces
            .iter()
            .map(|item| item.to_string())
            .collect(),
        stop_conditions: vec![
            "tabla nueva".to_string(),
            "endpoint nuevo".to_string(),
            "permiso nuevo".to_string(),
            "ruta nueva".to_string(),
            "IA diagnostica o protagonista".to_string(),
            "escritura clinica automatica".to_string(),
        ],
        execution_mode: execution_mode.to_string(),
        instructions: TRAINING_RULES.iter().map(|item| item.to_string()).collect(),
    }
}

fn training_scenario(id: &str) -> Result<TrainingScenario, String> {
    training_scenarios()
        .into_iter()
        .find(|scenario| scenario.id.eq_ignore_ascii_case(id))
        .ok_or_else(|| format!("Escenario TRAIN no registrado: {id}."))
}

fn common_training_blocks(
    inspection: &crate::agent::types::RepoInspection,
    scenario: &TrainingScenario,
    cycles: u8,
) -> Vec<String> {
    let mut blockers = Vec::new();
    if !inspection.is_git_repo {
        blockers.push("El proyecto objetivo debe ser un repo Git.".to_string());
    }
    if !inspection.is_one_epis {
        blockers.push("TRAIN solo se ejecuta con adaptador OneEpis activo.".to_string());
    }
    if cycles == 0 || cycles > MAX_TRAINING_CYCLES {
        blockers.push(format!(
            "TRAIN permite entre 1 y {MAX_TRAINING_CYCLES} ciclos concatenados; solicitado: {cycles}."
        ));
    }
    for gate in &scenario.gates {
        if gate.starts_with("check:") && !inspection.declared_gates.contains(gate) {
            blockers.push(format!(
                "Gate requerido no declarado en package.json: {gate}."
            ));
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
    scenario: &TrainingScenario,
    cycles: u8,
    blockers: Vec<String>,
) -> TrainingRun {
    TrainingRun {
        id: run_id(repo_path, &scenario.id),
        scenario_id: scenario.id.clone(),
        status: "blocked".to_string(),
        repo_path: repo_path.to_string(),
        branch: scenario.branch.clone(),
        cycles,
        blockers,
        warnings: Vec::new(),
        next_actions: vec![
            "Resolver bloqueos antes de preparar rama TRAIN.".to_string(),
            "No ejecutar entrenamiento ni consultar IA hasta que el plan este listo.".to_string(),
        ],
        no_push: true,
        local_ai_only: true,
        summary: format!("{} bloqueado.", scenario.id),
    }
}

fn ensure_training_branch(repo: &Path, branch: &str) -> Result<String, String> {
    let current_branch = git(repo, &["branch", "--show-current"])?;
    if current_branch == branch {
        return Ok(branch.to_string());
    }
    if git(repo, &["rev-parse", "--verify", branch]).is_ok() {
        git(repo, &["switch", branch])?;
    } else {
        let base = training_base_branch(repo, &current_branch);
        if current_branch != base {
            git(repo, &["switch", &base])?;
        }
        git(repo, &["switch", "-c", branch])?;
    }
    Ok(branch.to_string())
}

fn training_base_branch(repo: &Path, current_branch: &str) -> String {
    for candidate in ["main", "master"] {
        if git(repo, &["rev-parse", "--verify", candidate]).is_ok() {
            return candidate.to_string();
        }
    }
    current_branch.to_string()
}

fn run_id(repo_path: &str, scenario_id: &str) -> String {
    let seed = format!("{}:{}:{}", repo_path, scenario_id, Utc::now());
    let digest = sha256_hex(seed.as_bytes());
    format!("train-{}", &digest[..16])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    #[test]
    fn registry_contains_all_training_scenarios() {
        let scenarios = list_training_scenarios();
        assert_eq!(scenarios.len(), 15);
        let ids = scenarios
            .iter()
            .map(|scenario| scenario.id.as_str())
            .collect::<Vec<_>>();
        for index in 1..=15 {
            assert!(ids.contains(&format!("TRAIN-{index:03}").as_str()));
        }
        assert!(scenarios.iter().all(|scenario| scenario.local_ai_rule()));
    }

    #[test]
    fn plan_blocks_more_than_three_cycles() {
        let repo = temp_repo("cycles");
        let result = training_plan(TrainingRequest {
            repo_path: repo.display().to_string(),
            scenario_id: "TRAIN-001".to_string(),
            cycles: 4,
        })
        .expect("plan");
        assert_eq!(result.status, "blocked");
        assert!(result
            .blockers
            .iter()
            .any(|block| block.contains("3 ciclos")));
        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn plan_only_scenario_stays_plan_only() {
        let repo = temp_repo("plan-only");
        let result = training_plan(TrainingRequest {
            repo_path: repo.display().to_string(),
            scenario_id: "TRAIN-010".to_string(),
            cycles: 1,
        })
        .expect("plan");
        assert_eq!(result.status, "plan_only_ready");
        assert!(result.local_ai_only);
        assert!(result.no_push);
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("plan/contrato minimo")));
        let _ = fs::remove_dir_all(repo);
    }

    #[tokio::test]
    async fn prepare_creates_training_branch_without_changes() {
        let repo = temp_repo("prepare");
        let result = prepare_training_scenario(TrainingRequest {
            repo_path: repo.display().to_string(),
            scenario_id: "TRAIN-004".to_string(),
            cycles: 2,
        })
        .await
        .expect("prepare");
        assert_eq!(result.status, "ready_for_training");
        assert_eq!(result.branch, "agent/train-004-e2e-fragil");
        assert!(git(&repo, &["status", "--short"])
            .expect("status")
            .is_empty());
        let _ = fs::remove_dir_all(repo);
    }

    trait TrainingScenarioExt {
        fn local_ai_rule(&self) -> bool;
    }

    impl TrainingScenarioExt for TrainingScenario {
        fn local_ai_rule(&self) -> bool {
            self.instructions
                .iter()
                .any(|item| item.contains("Usar solo IA local"))
        }
    }

    fn temp_repo(label: &str) -> std::path::PathBuf {
        let repo = std::env::temp_dir().join(format!(
            "oneepis-agent-training-{label}-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(&repo).expect("repo dir");
        git(&repo, &["init", "-b", "main"]).expect("git init");
        git(&repo, &["config", "user.email", "agent@example.local"]).expect("git email");
        git(&repo, &["config", "user.name", "OneEpis Agent"]).expect("git name");
        fs::write(
            repo.join("package.json"),
            r#"{"scripts":{"check:api":"echo api","check:contract":"echo contract","check:size":"echo size","check:web":"echo web","check:e2e":"echo e2e"}}"#,
        )
        .expect("package");
        fs::write(repo.join("AGENTS.md"), "OneEpis local training fixture").expect("agents");
        fs::create_dir_all(repo.join("docs")).expect("docs");
        fs::write(repo.join("docs/GOVERNANCE.md"), "OneEpis governance").expect("gov");
        git(&repo, &["add", "."]).expect("add");
        let output = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .arg("commit")
            .arg("-m")
            .arg("init")
            .output()
            .expect("commit");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        repo
    }
}
