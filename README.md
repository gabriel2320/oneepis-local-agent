# OneEpis Local Agent

Asistente local de desarrollo para OneEpis, construido como herramienta externa con Tauri, React, Rust y Ollama.

Este repo no es OneEpis, no vive dentro de OneEpis y no reemplaza la gobernanza del repo clinico objetivo.

## Principios

- Ollama local es el unico proveedor IA.
- El agente lee gobernanza antes de proponer cambios.
- v0.2 genera `PatchDraft` revisable, sin escribir archivos del repo objetivo.
- v0.3 permite aplicar patches solo con repo Git limpio, rama local segura, token de confirmacion y gate declarado.
- No hay push automatico.
- El runner usa acciones tipadas, no shell libre generado por IA.
- Los registros se guardan en una base separada `oneepis_agent` cuando `AGENT_DATABASE_URL` esta configurado.

## Comandos

```bash
npm install
npm run dev
npm run check
```

CLI del agente:

```bash
npm run agent -- inspect "C:\\Users\\gdela\\OneDrive\\Documentos Importantes\\OneEpis"
npm run agent -- ollama
npm run agent -- plan "C:\\Users\\gdela\\OneDrive\\Documentos Importantes\\OneEpis" --objective "Auditar siguiente microciclo"
npm run agent -- draft "C:\\Users\\gdela\\OneDrive\\Documentos Importantes\\OneEpis" --objective "Preparar PatchDraft"
npm run agent -- gate "C:\\Users\\gdela\\OneDrive\\Documentos Importantes\\OneEpis" --gate check:size
npm run agent -- run "C:\\Users\\gdela\\OneDrive\\Documentos Importantes\\OneEpis" --max-cycles 1
npm run agent -- list-runs --limit 20
```

Aplicacion controlada de patches desde v0.3:

```bash
npm run agent -- review draft.json
npm run agent -- apply draft.json --confirm-token APPLY:draft-id --branch-strategy create_safe_branch
```

Los drafts generados por v0.2 quedan bloqueados por diseno. El comando `apply` existe para drafts concretos, no bloqueados, con diff real y revision aprobada.

## Configuracion

```bash
OLLAMA_BASE_URL=http://localhost:11434
AGENT_DATABASE_URL=postgresql://oneepis_agent:oneepis_agent@localhost:5444/oneepis_agent
AGENT_PRIMARY_CODE_MODEL=qwen2.5-coder:14b
AGENT_FAST_CODE_MODEL=qwen2.5-coder:7b
AGENT_GOVERNANCE_MODEL=qwen3:8b
AGENT_FALLBACK_MODEL=llama3.2:latest
AGENT_EMBEDDINGS_MODEL=bge-m3:latest
AGENT_MAX_CYCLES=3
```

Seleccion de modelo:

- El microplan gobernado usa `AGENT_GOVERNANCE_MODEL`.
- Los cambios de codigo se mantienen asociados a `AGENT_PRIMARY_CODE_MODEL`.
- OneEpis no es un LLM separado: es un perfil de repo detectado por gobernanza, documentos y gates.

PostgreSQL de desarrollo separado:

```bash
docker compose -f infra/docker-compose.dev.yml up -d
```

## GitHub

Repositorio recomendado:

```text
gabriel2320/oneepis-local-agent
```

Configuracion local esperada:

```bash
git remote add origin https://github.com/gabriel2320/oneepis-local-agent.git
git push -u origin main
```

El workflow `.github/workflows/ci.yml` ejecuta `npm run check` en cada push a `main` y en pull requests.

## Estados Del Ciclo

```text
preflight -> governance_read -> repo_audit -> micro_plan -> patch_draft
-> safety_review -> apply_patch -> gate_run -> result_record
-> lesson_record -> stop_or_next
```

## Roadmap

La hoja de ruta esta en `docs/DEVELOPMENT_PLAN.md`.

Estado actual:

- Inspeccion de repo objetivo.
- Deteccion OneEpis por `AGENTS.md` + `docs/GOVERNANCE.md`.
- Estado Ollama y modelos por politica.
- Plan de microciclo gobernado.
- `PatchDraft` estructurado y bloqueado por defecto.
- Revision deterministica de drafts.
- Gates declarados por `package.json`.
- Bitacora PostgreSQL opcional.
- Runner dry-run con maquina de estados cerrada.
