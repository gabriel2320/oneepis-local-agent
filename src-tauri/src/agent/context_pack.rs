use crate::agent::safety::{sanitize_log, sha256_hex};
use crate::agent::types::{ContextPackFile, DevelopmentContextPack, DevelopmentWorkPackage};
use crate::agent::work_package::development_work_package;
use std::fs;
use std::path::{Component, Path, PathBuf};

const MAX_FILE_BYTES: usize = 64 * 1024;
const MAX_TOTAL_BYTES: usize = 24 * 1024;
const MAX_EXCERPT_BYTES: usize = 6 * 1024;
const MIN_EXCERPT_BYTES: usize = 512;
const MAX_DIRECTORY_FILES: usize = 8;
const MAX_DIRECTORY_DEPTH: usize = 2;

pub async fn development_context_pack(
    repo_path: &str,
    objective: &str,
    base_url: Option<String>,
) -> Result<DevelopmentContextPack, String> {
    let package = development_work_package(repo_path, objective, base_url).await?;
    Ok(build_development_context_pack(
        Path::new(repo_path),
        &package,
    ))
}

pub fn build_development_context_pack(
    repo_root: &Path,
    package: &DevelopmentWorkPackage,
) -> DevelopmentContextPack {
    let mut builder = ContextPackBuilder {
        repo_root,
        files: Vec::new(),
        warnings: package.warnings.clone(),
        total_bytes: 0,
    };

    if package.status == "blocked" {
        builder.warnings.push(
            "Paquete bloqueado: el contexto es solo diagnostico y no habilita PatchDraft."
                .to_string(),
        );
    }

    for raw_path in &package.files_to_inspect {
        builder.add_requested_path(raw_path);
    }

    let status = if package.status == "blocked" {
        "blocked"
    } else if builder.files.is_empty() || !builder.warnings.is_empty() {
        "partial"
    } else {
        "ready"
    };

    DevelopmentContextPack {
        repo_path: package.repo_path.clone(),
        objective: package.objective.clone(),
        status: status.to_string(),
        summary: summary_for(status, package, builder.files.len(), builder.total_bytes),
        files: builder.files,
        warnings: builder.warnings,
        prompt_notes: prompt_notes(),
        gates: package.gates.clone(),
        total_bytes: builder.total_bytes,
        max_bytes: MAX_TOTAL_BYTES,
    }
}

struct ContextPackBuilder<'a> {
    repo_root: &'a Path,
    files: Vec<ContextPackFile>,
    warnings: Vec<String>,
    total_bytes: usize,
}

impl ContextPackBuilder<'_> {
    fn add_requested_path(&mut self, raw_path: &str) {
        if self.total_bytes >= MAX_TOTAL_BYTES {
            self.warnings.push(format!(
                "Presupuesto de contexto agotado antes de leer {raw_path}."
            ));
            return;
        }

        let Some(path) = safe_join(self.repo_root, raw_path) else {
            self.warnings.push(format!(
                "Ruta omitida por salir del repo objetivo: {raw_path}."
            ));
            return;
        };

        if is_sensitive_path(raw_path) {
            self.warnings.push(format!(
                "Ruta sensible omitida del contexto local: {raw_path}."
            ));
            self.files.push(ContextPackFile {
                path: normalize_path(raw_path),
                kind: "skipped".to_string(),
                bytes: 0,
                lines: 0,
                sha256: String::new(),
                summary: "Omitido por posible secreto, PHI o configuracion local.".to_string(),
                excerpt: String::new(),
            });
            return;
        }

        match fs::metadata(&path) {
            Ok(metadata) if metadata.is_dir() => self.add_directory(raw_path, &path),
            Ok(metadata) if metadata.is_file() => self.add_file(raw_path, &path, metadata.len()),
            Ok(_) => self.warnings.push(format!(
                "Ruta omitida porque no es archivo ni directorio: {raw_path}."
            )),
            Err(_) => self.files.push(ContextPackFile {
                path: normalize_path(raw_path),
                kind: "missing".to_string(),
                bytes: 0,
                lines: 0,
                sha256: String::new(),
                summary: "No existe en el repo objetivo; revisar si el paquete debe ajustarse."
                    .to_string(),
                excerpt: String::new(),
            }),
        }
    }

    fn add_directory(&mut self, raw_path: &str, path: &Path) {
        let mut candidates = Vec::new();
        collect_text_candidates(path, self.repo_root, 0, &mut candidates);
        candidates.sort();
        candidates.truncate(MAX_DIRECTORY_FILES);

        let listing = if candidates.is_empty() {
            "Sin archivos de texto candidatos dentro del limite.".to_string()
        } else {
            candidates.join("\n")
        };
        self.files.push(ContextPackFile {
            path: normalize_path(raw_path),
            kind: "directory".to_string(),
            bytes: listing.len(),
            lines: listing.lines().count(),
            sha256: sha256_hex(listing.as_bytes()),
            summary: format!(
                "Directorio resumido; se inspeccionan hasta {} archivos candidatos.",
                MAX_DIRECTORY_FILES
            ),
            excerpt: listing,
        });

        for relative in candidates {
            if !self.has_excerpt_budget() {
                self.warnings.push(format!(
                    "Presupuesto de contexto agotado dentro del directorio {raw_path}."
                ));
                break;
            }
            if let Some(child) = safe_join(self.repo_root, &relative) {
                if let Ok(metadata) = fs::metadata(&child) {
                    self.add_file(&relative, &child, metadata.len());
                }
            }
        }
    }

    fn add_file(&mut self, raw_path: &str, path: &Path, byte_len: u64) {
        let byte_len = byte_len as usize;
        if byte_len > MAX_FILE_BYTES {
            self.warnings.push(format!(
                "Archivo omitido por tamano: {} ({} bytes).",
                raw_path, byte_len
            ));
            self.files.push(ContextPackFile {
                path: normalize_path(raw_path),
                kind: "skipped".to_string(),
                bytes: byte_len,
                lines: 0,
                sha256: String::new(),
                summary: "Omitido porque supera el limite por archivo del context pack."
                    .to_string(),
                excerpt: String::new(),
            });
            return;
        }

        let Ok(bytes) = fs::read(path) else {
            self.warnings.push(format!(
                "No se pudo leer {raw_path}; se omite del contexto."
            ));
            return;
        };

        if looks_binary(&bytes) {
            self.warnings
                .push(format!("Archivo binario omitido del contexto: {raw_path}."));
            self.files.push(ContextPackFile {
                path: normalize_path(raw_path),
                kind: "skipped".to_string(),
                bytes: byte_len,
                lines: 0,
                sha256: sha256_hex(&bytes),
                summary: "Omitido porque no parece texto seguro para el modelo local.".to_string(),
                excerpt: String::new(),
            });
            return;
        }

        let text = sanitize_log(&String::from_utf8_lossy(&bytes));
        if !self.has_excerpt_budget() {
            self.warnings.push(format!(
                "Archivo referenciado sin extracto por presupuesto de contexto: {raw_path}."
            ));
            self.files.push(ContextPackFile {
                path: normalize_path(raw_path),
                kind: "file".to_string(),
                bytes: byte_len,
                lines: text.lines().count(),
                sha256: sha256_hex(&bytes),
                summary: summary_for_file(raw_path, &text),
                excerpt: String::new(),
            });
            return;
        }

        let remaining = MAX_TOTAL_BYTES.saturating_sub(self.total_bytes);
        let excerpt = truncate_to_budget(&text, remaining.min(MAX_EXCERPT_BYTES));
        self.total_bytes += excerpt.len();
        if excerpt.len() < text.len() {
            self.warnings.push(format!(
                "Extracto truncado por presupuesto de contexto: {raw_path}."
            ));
        }

        self.files.push(ContextPackFile {
            path: normalize_path(raw_path),
            kind: "file".to_string(),
            bytes: byte_len,
            lines: text.lines().count(),
            sha256: sha256_hex(&bytes),
            summary: summary_for_file(raw_path, &text),
            excerpt,
        });
    }

    fn has_excerpt_budget(&self) -> bool {
        MAX_TOTAL_BYTES.saturating_sub(self.total_bytes) >= MIN_EXCERPT_BYTES
    }
}

fn collect_text_candidates(
    dir: &Path,
    repo_root: &Path,
    depth: usize,
    candidates: &mut Vec<String>,
) {
    if depth > MAX_DIRECTORY_DEPTH || candidates.len() >= MAX_DIRECTORY_FILES {
        return;
    }

    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        if candidates.len() >= MAX_DIRECTORY_FILES {
            return;
        }
        let path = entry.path();
        let relative = path
            .strip_prefix(repo_root)
            .map(normalize_path_buf)
            .unwrap_or_else(|_| normalize_path_buf(&path));
        if is_sensitive_path(&relative) {
            continue;
        }
        if path.is_dir() {
            if should_skip_dir(&path) {
                continue;
            }
            collect_text_candidates(&path, repo_root, depth + 1, candidates);
        } else if is_supported_text_path(&path) {
            candidates.push(relative);
        }
    }
}

fn safe_join(root: &Path, raw_path: &str) -> Option<PathBuf> {
    let path = Path::new(raw_path);
    if path.is_absolute() {
        return None;
    }
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::Prefix(_) | Component::RootDir
        )
    }) {
        return None;
    }
    Some(root.join(path))
}

fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(1024).any(|byte| *byte == 0) || std::str::from_utf8(bytes).is_err()
}

fn truncate_to_budget(input: &str, max_bytes: usize) -> String {
    if max_bytes < 13 {
        return String::new();
    }
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

fn summary_for(
    status: &str,
    package: &DevelopmentWorkPackage,
    files: usize,
    bytes: usize,
) -> String {
    match status {
        "blocked" => format!(
            "Contexto diagnostico bloqueado para '{}'; resolver readiness antes de aplicar.",
            package.objective
        ),
        "partial" => format!(
            "Contexto parcial para '{}': {} entradas, {} bytes sanitizados.",
            package.objective, files, bytes
        ),
        _ => format!(
            "Contexto listo para '{}': {} entradas, {} bytes sanitizados.",
            package.objective, files, bytes
        ),
    }
}

fn summary_for_file(path: &str, text: &str) -> String {
    let lines = text.lines().count();
    format!(
        "{} lineas de texto sanitizado para orientar al modelo local sobre {}.",
        lines,
        normalize_path(path)
    )
}

fn prompt_notes() -> Vec<String> {
    vec![
        "Usar solo modelos Ollama locales; no enviar este contexto a servicios externos.".to_string(),
        "No incluir PHI, secretos, emails ni identificadores reales en respuestas, tests o diffs."
            .to_string(),
        "Aplicar la escalera OneEpis: paciente -> ficha -> papel -> API -> PostgreSQL -> auditoria -> permisos -> OpenAPI.".to_string(),
        "Responder con microdiff revisable, gates oficiales y condicion de parada explicita."
            .to_string(),
        "Si falta contexto, pedir inspeccion adicional antes de inventar comportamiento clinico."
            .to_string(),
    ]
}

fn should_skip_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| {
            matches!(
                name,
                ".git"
                    | "node_modules"
                    | "target"
                    | "dist"
                    | "build"
                    | ".next"
                    | ".venv"
                    | "__pycache__"
            )
        })
        .unwrap_or(false)
}

fn is_supported_text_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "md" | "txt"
                    | "ts"
                    | "tsx"
                    | "js"
                    | "jsx"
                    | "mjs"
                    | "json"
                    | "rs"
                    | "py"
                    | "sql"
                    | "toml"
                    | "yml"
                    | "yaml"
                    | "css"
                    | "html"
                    | "openapi"
            )
        })
        .unwrap_or(false)
}

fn is_sensitive_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.contains(".env")
        || lower.contains("secret")
        || lower.contains("credential")
        || lower.contains("private-key")
        || lower.contains("id_rsa")
        || lower.contains("id_ed25519")
        || lower.ends_with(".pem")
        || lower.ends_with(".pfx")
        || lower.ends_with(".key")
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn normalize_path_buf(path: &Path) -> String {
    normalize_path(&path.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::WorkPackageTest;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn context_pack_reads_and_sanitizes_small_files() {
        let root = temp_repo("reads_and_sanitizes");
        fs::write(
            root.join("AGENTS.md"),
            "token=abc123 test@example.com 12.345.678-9\nRegla OneEpis",
        )
        .expect("write agent file");

        let package = package_for(&root, vec!["AGENTS.md".to_string(), ".env".to_string()]);
        let pack = build_development_context_pack(&root, &package);

        assert_eq!(pack.status, "partial");
        assert_eq!(pack.files[0].kind, "file");
        assert!(pack.files[0].excerpt.contains("[REDACTED]"));
        assert!(pack.files[0].excerpt.contains("[REDACTED_EMAIL]"));
        assert!(pack.files[0].excerpt.contains("[REDACTED_ID]"));
        assert!(!pack.files[0].excerpt.contains("abc123"));
        assert!(pack
            .warnings
            .iter()
            .any(|warning| warning.contains("Ruta sensible")));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn context_pack_skips_large_and_unsafe_paths() {
        let root = temp_repo("skips_large");
        let large = "x".repeat(MAX_FILE_BYTES + 1);
        fs::write(root.join("large.md"), large).expect("write large file");

        let package = package_for(
            &root,
            vec!["large.md".to_string(), "../outside.md".to_string()],
        );
        let pack = build_development_context_pack(&root, &package);

        assert_eq!(pack.status, "partial");
        assert!(pack.files.iter().any(|file| file.kind == "skipped"));
        assert!(pack
            .warnings
            .iter()
            .any(|warning| warning.contains("salir del repo")));
        assert!(pack.total_bytes <= pack.max_bytes);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn context_pack_summarizes_directory_candidates() {
        let root = temp_repo("summarizes_directory");
        fs::create_dir_all(root.join("docs")).expect("create docs");
        fs::write(root.join("docs").join("GOVERNANCE.md"), "Gobernanza").expect("write docs");
        fs::write(root.join("docs").join("SCREEN_TREE.md"), "Pantallas").expect("write docs");

        let package = package_for(&root, vec!["docs".to_string()]);
        let pack = build_development_context_pack(&root, &package);

        assert!(pack.files.iter().any(|file| file.kind == "directory"));
        assert!(pack
            .files
            .iter()
            .any(|file| file.path == "docs/GOVERNANCE.md"));

        let _ = fs::remove_dir_all(root);
    }

    fn package_for(root: &Path, files_to_inspect: Vec<String>) -> DevelopmentWorkPackage {
        DevelopmentWorkPackage {
            repo_path: root.to_string_lossy().to_string(),
            title: "Paquete".to_string(),
            objective: "Reducir archivo clinico near-limit".to_string(),
            status: "ready_to_draft".to_string(),
            summary: "Listo".to_string(),
            branch_strategy: "agent/reducir".to_string(),
            files_to_inspect,
            implementation_steps: Vec::new(),
            test_plan: vec![WorkPackageTest {
                gate: "check:size".to_string(),
                command: "npm run check:size".to_string(),
                purpose: "Validar tamano".to_string(),
                required: true,
            }],
            acceptance_criteria: Vec::new(),
            stop_conditions: Vec::new(),
            gates: vec!["check:size".to_string()],
            warnings: Vec::new(),
            can_draft: true,
            can_apply: false,
        }
    }

    fn temp_repo(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("oneepis-context-{name}-{stamp}"));
        fs::create_dir_all(&root).expect("create temp root");
        root
    }
}
