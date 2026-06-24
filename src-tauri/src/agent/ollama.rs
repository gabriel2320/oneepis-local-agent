use crate::agent::safety::sanitize_log;
use crate::agent::types::{
    LocalModelProposal, MicroPlan, ModelPolicy, OllamaModel, OllamaStatus, RepoInspection,
};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;

#[derive(Debug, Deserialize)]
struct TagsResponse {
    models: Vec<TagModel>,
}

#[derive(Debug, Deserialize)]
struct TagModel {
    name: String,
    size: Option<u64>,
    details: Option<TagDetails>,
}

#[derive(Debug, Deserialize)]
struct TagDetails {
    family: Option<String>,
    parameter_size: Option<String>,
    quantization_level: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    content: String,
}

pub async fn get_ollama_status(base_url: Option<String>) -> Result<OllamaStatus, String> {
    let base_url = normalize_base_url(base_url);
    let policy = ModelPolicy::default();
    let client = Client::new();
    let response = client.get(format!("{base_url}/api/tags")).send().await;

    let Ok(response) = response else {
        return Ok(unavailable_status(base_url, policy, "Ollama no respondio."));
    };
    if !response.status().is_success() {
        return Ok(unavailable_status(
            base_url,
            policy,
            &format!("Ollama respondio con estado HTTP {}.", response.status()),
        ));
    }

    let body = response
        .json::<TagsResponse>()
        .await
        .map_err(|err| format!("No se pudo leer /api/tags: {err}"))?;
    let models: Vec<OllamaModel> = body
        .models
        .into_iter()
        .map(|model| {
            let details = model.details;
            OllamaModel {
                name: model.name,
                size: model.size.unwrap_or_default(),
                family: details
                    .as_ref()
                    .and_then(|item| item.family.clone())
                    .unwrap_or_default(),
                parameters: details
                    .as_ref()
                    .and_then(|item| item.parameter_size.clone())
                    .unwrap_or_default(),
                quantization: details
                    .and_then(|item| item.quantization_level)
                    .unwrap_or_default(),
            }
        })
        .collect();
    let missing_policy_models = missing_policy_models(&policy, &models);
    let message = if missing_policy_models.is_empty() {
        "Ollama activo y modelos de politica disponibles.".to_string()
    } else {
        format!(
            "Ollama activo; faltan modelos de politica: {}.",
            missing_policy_models.join(", ")
        )
    };

    Ok(OllamaStatus {
        base_url,
        available: true,
        message,
        models,
        policy,
        missing_policy_models,
    })
}

pub async fn ask_for_micro_plan(
    base_url: Option<String>,
    inspection: &RepoInspection,
    objective: &str,
) -> Option<MicroPlan> {
    let status = get_ollama_status(base_url.clone()).await.ok()?;
    if !status.available {
        return None;
    }
    let model = choose_planning_model(&status);
    let repo_profile = if inspection.is_one_epis {
        "oneepis"
    } else {
        "generic"
    };
    let prompt = json!({
        "repo": inspection.project_name,
        "is_oneepis": inspection.is_one_epis,
        "repo_profile": repo_profile,
        "branch": inspection.current_branch,
        "dirty": inspection.dirty,
        "declared_gates": inspection.declared_gates,
        "rules": inspection.detected_rules,
        "blocks": inspection.blocks,
        "objective": objective
    });
    let client = Client::new();
    let response = client
        .post(format!("{}/api/chat", status.base_url))
        .json(&json!({
            "model": model,
            "stream": false,
            "think": false,
            "format": "json",
            "messages": [
                {
                    "role": "system",
                    "content": "Eres el planificador local gobernado de OneEpis Local Agent usando modelos Ollama existentes. Usa el modelo de gobernanza para evaluar reglas y producir un microplan. Si repo_profile es oneepis, OneEpis es un repo objetivo permitido: aplica su doctrina sin rechazar el trabajo por defecto. Debes preferir paciente/ficha/papel/API/PostgreSQL/auditoria/permisos/OpenAPI, cambios pequenos y gates oficiales. Debes marcar blocked=true solo ante bloqueo duro: repo sucio, repo no Git, riesgo red, falta de gobernanza necesaria, falta de gate minimo o peticion fuera de limites activos como dashboard central, RAG amplio, firma, receta o IA protagonista sin plan explicito. Advertencias de gobernanza no son bloqueo. Devuelve solo JSON compacto con objective, recommendedGate, riskLevel, touchedSurfaces, requiredGates, steps, warnings, blocked. Usa riskLevel green, yellow o red. No propongas push, shell libre ni cambios fuera de gobernanza."
                },
                {
                    "role": "user",
                    "content": prompt.to_string()
                }
            ],
            "options": {
                "temperature": 0.1,
                "num_predict": 600
            }
        }))
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let chat = response.json::<ChatResponse>().await.ok()?;
    let mut plan = parse_plan_content(&chat.message.content)?;
    plan.model_used = model;
    Some(plan)
}

pub async fn ask_for_development_proposal(
    base_url: Option<String>,
    system_prompt: &str,
    user_prompt: &str,
) -> LocalModelProposal {
    let status = match get_ollama_status(base_url.clone()).await {
        Ok(status) if status.available => status,
        Ok(status) => {
            return unavailable_proposal("ollama_unavailable", &status.message);
        }
        Err(err) => {
            return unavailable_proposal("ollama_error", &err);
        }
    };
    let model = choose_coding_model(&status);
    let client = Client::new();
    let response = client
        .post(format!("{}/api/chat", status.base_url))
        .json(&json!({
            "model": model,
            "stream": false,
            "think": false,
            "format": "json",
            "messages": [
                {
                    "role": "system",
                    "content": system_prompt
                },
                {
                    "role": "user",
                    "content": user_prompt
                }
            ],
            "options": {
                "temperature": 0.1,
                "num_predict": 900
            }
        }))
        .send()
        .await;

    let Ok(response) = response else {
        return unavailable_proposal("ollama_request_failed", "No se pudo consultar Ollama.");
    };
    if !response.status().is_success() {
        return unavailable_proposal(
            "ollama_http_error",
            &format!("Ollama respondio con HTTP {}.", response.status()),
        );
    }
    let chat = match response.json::<ChatResponse>().await {
        Ok(chat) => chat,
        Err(err) => {
            return unavailable_proposal(
                "ollama_parse_error",
                &format!("No se pudo leer respuesta de Ollama: {err}."),
            );
        }
    };
    parse_development_proposal_content(&chat.message.content, &model)
}

fn parse_plan_content(content: &str) -> Option<MicroPlan> {
    let value = serde_json::from_str::<Value>(content).ok()?;
    let candidate = value.get("plan").cloned().unwrap_or(value);
    if let Ok(plan) = serde_json::from_value::<MicroPlan>(candidate.clone()) {
        return Some(plan);
    }

    let objective = candidate
        .get("objective")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let recommended_gate = candidate
        .get("recommendedGate")
        .or_else(|| candidate.get("recommended_gate"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let risk_level = candidate
        .get("riskLevel")
        .or_else(|| candidate.get("risk_level"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let touched_surfaces = string_list(
        candidate
            .get("touchedSurfaces")
            .or_else(|| candidate.get("touched_surfaces")),
    );
    let required_gates = string_list(
        candidate
            .get("requiredGates")
            .or_else(|| candidate.get("required_gates")),
    );
    let steps = string_list(candidate.get("steps"));
    let mut warnings = string_list(candidate.get("warnings"));
    let blocked = match candidate.get("blocked") {
        Some(Value::Bool(value)) => *value,
        Some(Value::Array(values)) => {
            warnings.extend(
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToString::to_string),
            );
            !values.is_empty()
        }
        Some(Value::String(value)) => {
            if !value.is_empty() {
                warnings.push(value.clone());
            }
            !value.is_empty()
        }
        _ => false,
    };

    Some(MicroPlan {
        objective,
        recommended_gate,
        risk_level,
        touched_surfaces,
        required_gates,
        steps,
        warnings,
        blocked,
        model_used: String::new(),
    })
}

fn parse_development_proposal_content(content: &str, model: &str) -> LocalModelProposal {
    let sanitized = sanitize_log(content);
    let value = serde_json::from_str::<Value>(&sanitized).unwrap_or(Value::Null);
    let candidate = value.get("proposal").cloned().unwrap_or(value);
    let summary = candidate
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or("Propuesta local sin resumen estructurado.")
        .to_string();
    let files_to_change = string_list(
        candidate
            .get("filesToChange")
            .or_else(|| candidate.get("files_to_change")),
    );
    let implementation_notes = string_list(
        candidate
            .get("implementationNotes")
            .or_else(|| candidate.get("implementation_notes")),
    );
    let risks = string_list(candidate.get("risks"));
    let gates = string_list(candidate.get("gates"));

    LocalModelProposal {
        status: "proposed".to_string(),
        model_used: model.to_string(),
        summary,
        files_to_change,
        implementation_notes,
        risks,
        gates,
        raw_response: sanitized,
    }
}

fn unavailable_proposal(status: &str, summary: &str) -> LocalModelProposal {
    LocalModelProposal {
        status: status.to_string(),
        model_used: "none".to_string(),
        summary: sanitize_log(summary),
        files_to_change: Vec::new(),
        implementation_notes: Vec::new(),
        risks: vec!["Sin propuesta de modelo local; usar brief determinista.".to_string()],
        gates: Vec::new(),
        raw_response: String::new(),
    }
}

fn string_list(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .map(ToString::to_string)
            .collect(),
        Some(Value::String(value)) if !value.is_empty() => vec![value.clone()],
        _ => Vec::new(),
    }
}

fn normalize_base_url(base_url: Option<String>) -> String {
    base_url
        .or_else(|| std::env::var("OLLAMA_BASE_URL").ok())
        .unwrap_or_else(|| "http://localhost:11434".to_string())
        .trim_end_matches('/')
        .to_string()
}

fn unavailable_status(base_url: String, policy: ModelPolicy, message: &str) -> OllamaStatus {
    OllamaStatus {
        base_url,
        available: false,
        message: message.to_string(),
        models: Vec::new(),
        missing_policy_models: vec![
            policy.primary_code.clone(),
            policy.fast_code.clone(),
            policy.governance.clone(),
            policy.fallback.clone(),
            policy.embeddings.clone(),
        ],
        policy,
    }
}

fn missing_policy_models(policy: &ModelPolicy, models: &[OllamaModel]) -> Vec<String> {
    let names: BTreeSet<&str> = models.iter().map(|model| model.name.as_str()).collect();
    [
        &policy.primary_code,
        &policy.fast_code,
        &policy.governance,
        &policy.fallback,
        &policy.embeddings,
    ]
    .into_iter()
    .filter(|name| !names.contains(name.as_str()))
    .cloned()
    .collect()
}

fn choose_planning_model(status: &OllamaStatus) -> String {
    let available: BTreeSet<&str> = status
        .models
        .iter()
        .map(|model| model.name.as_str())
        .collect();
    if available.contains(status.policy.governance.as_str()) {
        status.policy.governance.clone()
    } else if available.contains(status.policy.fallback.as_str()) {
        status.policy.fallback.clone()
    } else {
        status
            .models
            .first()
            .map(|model| model.name.clone())
            .unwrap_or_else(|| status.policy.fallback.clone())
    }
}

fn choose_coding_model(status: &OllamaStatus) -> String {
    let available: BTreeSet<&str> = status
        .models
        .iter()
        .map(|model| model.name.as_str())
        .collect();
    if available.contains(status.policy.primary_code.as_str()) {
        status.policy.primary_code.clone()
    } else if available.contains(status.policy.fast_code.as_str()) {
        status.policy.fast_code.clone()
    } else if available.contains(status.policy.governance.as_str()) {
        status.policy.governance.clone()
    } else if available.contains(status.policy.fallback.as_str()) {
        status.policy.fallback.clone()
    } else {
        status
            .models
            .first()
            .map(|model| model.name.clone())
            .unwrap_or_else(|| status.policy.fallback.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_development_proposal_json_contract() {
        let proposal = parse_development_proposal_content(
            r#"{
                "summary": "Extraer regla pequena",
                "filesToChange": ["apps/api/src/service.py"],
                "implementationNotes": ["Mover funcion pura"],
                "risks": ["Archivo near-limit"],
                "gates": ["check:size"]
            }"#,
            "qwen3:8b",
        );

        assert_eq!(proposal.status, "proposed");
        assert_eq!(proposal.model_used, "qwen3:8b");
        assert!(proposal
            .files_to_change
            .contains(&"apps/api/src/service.py".to_string()));
        assert!(proposal.gates.contains(&"check:size".to_string()));
    }

    #[test]
    fn unavailable_proposal_is_safe_fallback() {
        let proposal = unavailable_proposal("ollama_unavailable", "token=abc");

        assert_eq!(proposal.status, "ollama_unavailable");
        assert!(proposal.summary.contains("[REDACTED]"));
        assert!(proposal.files_to_change.is_empty());
    }
}
