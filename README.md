# OneEpis Local Agent

Asistente local de desarrollo para OneEpis, construido como herramienta externa con Tauri, React, Rust y Ollama.

Este repo no es OneEpis, no vive dentro de OneEpis y no reemplaza la gobernanza del repo clinico objetivo.

## Principios

- Ollama local es el unico proveedor IA.
- El agente lee gobernanza antes de proponer cambios.
- La interfaz traduce estado tecnico a lenguaje natural en espanol: que hace, por que, siguiente accion y limite de gobernanza.
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

Acceso directo de escritorio:

```bat
scripts\launch-dev.cmd -SmokeTest
```

El `.lnk` del Escritorio debe apuntar a `C:\Windows\System32\cmd.exe` con argumentos:

```text
/k call "C:\Users\gdela\OneDrive\Documentos Importantes\oneepis-local-agent\scripts\launch-dev.cmd"
```

El launcher `.cmd` delega en `scripts\launch-dev.ps1` con `-ExecutionPolicy Bypass`, mantiene la ventana abierta y muestra cualquier error antes de cerrar.

CLI del agente:

```bash
npm run agent -- inspect "C:\\Users\\gdela\\OneDrive\\Documentos Importantes\\OneEpis"
npm run agent -- readiness "C:\\Users\\gdela\\OneDrive\\Documentos Importantes\\OneEpis"
npm run agent -- work-package "C:\\Users\\gdela\\OneDrive\\Documentos Importantes\\OneEpis" --objective "Reducir un archivo clinico near-limit"
npm run agent -- context-pack "C:\\Users\\gdela\\OneDrive\\Documentos Importantes\\OneEpis" --objective "Reducir un archivo clinico near-limit"
npm run agent -- brief "C:\\Users\\gdela\\OneDrive\\Documentos Importantes\\OneEpis" --objective "Reducir un archivo clinico near-limit" --ask-model
npm run agent -- decision "C:\\Users\\gdela\\OneDrive\\Documentos Importantes\\OneEpis" --objective "Reducir un archivo clinico near-limit" --ask-model
npm run agent -- evolution-plan "C:\\Users\\gdela\\OneDrive\\Documentos Importantes\\OneEpis" --objective "Elegir el siguiente microproceso supervisado"
npm run agent -- ollama
npm run agent -- plan "C:\\Users\\gdela\\OneDrive\\Documentos Importantes\\OneEpis" --objective "Auditar siguiente microciclo"
npm run agent -- draft "C:\\Users\\gdela\\OneDrive\\Documentos Importantes\\OneEpis" --objective "Preparar PatchDraft"
npm run agent -- gate "C:\\Users\\gdela\\OneDrive\\Documentos Importantes\\OneEpis" --gate check:size
npm run agent -- run "C:\\Users\\gdela\\OneDrive\\Documentos Importantes\\OneEpis" --max-cycles 1
npm run agent -- report "C:\\Users\\gdela\\OneDrive\\Documentos Importantes\\OneEpis" --objective "Preparar reporte PR gobernado"
npm run agent -- list-runs --limit 20
```

Aplicacion controlada de patches desde v0.3:

```bash
npm run agent -- review draft.json
npm run agent -- prepare-apply draft.json
npm run agent -- apply draft.json --confirm-token APPLY:draft-id --branch-strategy create_safe_branch
```

Los drafts generados por v0.2 quedan bloqueados por diseno. `prepare-apply` no escribe archivos: informa si el draft esta bloqueado, listo para confirmacion o listo para apply en rama segura `agent/<objetivo>`. El comando `apply` existe para drafts concretos, no bloqueados, con diff real, revision aprobada, token humano y rama segura.

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
preflight -> governance_read -> repo_audit -> evolution_plan -> work_package
-> context_pack -> development_brief -> implementation_decision -> micro_plan -> patch_draft
-> safety_review -> apply_patch -> gate_run -> result_record
-> lesson_record -> stop_or_next
```

La capa de evolucion supervisada se ejecuta antes de convertir el objetivo en paquete/patch: puntua candidatos, elige un solo microproceso local y deja visible si el resultado es `ready`, `review_only` o `blocked`.

## Adaptador OneEpis

Cuando el repo objetivo es OneEpis, el agente aplica reglas deterministicas sobre el microplan antes de continuar:

- clasifica el objetivo contra el semaforo de gobernanza;
- bloquea alcances rojos como dashboard central, chat libre, RAG amplio, IA externa, receta valida, firma clinica u ordenes ejecutables;
- agrega gates oficiales segun superficie: `check:api`, `check:web`, `check:contract`, `check:e2e` o `check:size`;
- convierte advertencias blandas de gobernanza en warnings, no en rechazo automatico.

## Lenguaje Natural Y Ayudas

La pantalla principal mantiene una voz operativa del agente:

- explica el estado actual en espanol simple;
- muestra la siguiente accion concreta;
- muestra la baranda de gobernanza que limita la accion;
- resume que poder local puede usar en ese punto del ciclo;
- evita esconder el detalle tecnico: plan, PatchDraft, revision, gates y bitacora siguen visibles.
- muestra contexto local sanitizado: archivos incluidos, omisiones, presupuesto de bytes y notas para el modelo Ollama.
- convierte el contexto en un `DevelopmentBrief`: prompts, contrato JSON, propuesta local opcional y condiciones de parada.
- convierte la propuesta local en una `ImplementationDecision`: una sola intencion lista para PatchDraft o un bloqueo explicito.
- genera un `AgentRunReport` en Markdown para PR: estados, checklist, warnings, acciones siguientes, gate recomendado y lecciones del microproceso.
- calcula un `EvolutionPlan`: ranking de candidatos, puntaje neto, veredicto, frontera local y siguiente microproceso recomendado.
- repara propuestas locales incompletas infiriendo hasta 3 archivos seguros desde el contexto gobernado cuando Ollama omite `filesToChange`.

Este sistema no aumenta permisos por fuera de gobernanza. Da mas claridad y coordina mejor los ciclos cerrados:

```text
inspeccionar -> evolucion -> paquete -> contexto -> brief IA -> planificar
-> decision -> preparar PatchDraft -> revisar safety -> ejecutar gate declarado
-> registrar resultado -> detener
```

## Roadmap

La hoja de ruta esta en `docs/DEVELOPMENT_PLAN.md`.

Estado actual:

- Inspeccion de repo objetivo.
- Deteccion OneEpis por `AGENTS.md` + `docs/GOVERNANCE.md`.
- Estado Ollama y modelos por politica.
- Plan de microciclo gobernado.
- `PatchDraft` estructurado, revisable por defecto y bloqueado solo ante bloqueo duro.
- Revision deterministica de drafts.
- Diagnostico `DevelopmentReadiness` con bloqueos, warnings, acciones siguientes y microciclos sugeridos.
- `DevelopmentWorkPackage` con archivos a inspeccionar, pasos, plan de pruebas, criterios de aceptacion y condiciones de parada.
- `DevelopmentContextPack` con extractos locales sanitizados, limites de bytes, warnings, gates y notas de prompt para Ollama.
- `DevelopmentBrief` con orden de trabajo, prompts, contrato de respuesta y propuesta estructurada opcional desde Ollama.
- `ImplementationDecision` con archivos seleccionados, pasos, gates, aceptacion, bloqueos y siguiente accion antes de PatchDraft.
- `EvolutionPlan` con candidato seleccionado, ranking puntuado, dimensiones, veredicto, bloqueos, warnings y frontera local sin escritura.
- `AgentRunReport` con Markdown revisable para PR y microprocesos cerrados.
- `ApplyReadiness` para prevalidar apply v0.3 sin escribir: token requerido, rama segura, checks, bloqueos y siguientes acciones.
- Gates declarados por `package.json`.
- Lenguaje natural de estado, ayudas accionables y autonomia gobernada visible en UI.
- Bitacora PostgreSQL opcional.
- Runner dry-run con maquina de estados cerrada que registra paquete, contexto, brief, decision, plan, PatchDraft, safety y parada.
