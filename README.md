# OneEpis Local Agent

Agente local para una sola tarea: clonar OneEpis en Windows, auditarlo con IA local cuando Ollama esta disponible, elegir el siguiente trabajo verificable y ejecutarlo con herramientas locales.

Este repo es una herramienta externa. No es OneEpis, no vive dentro de OneEpis y no reemplaza la gobernanza del repo objetivo.

## Principios

- Ollama local es el unico proveedor IA; si no esta disponible, se usa un plan deterministico local.
- El agente lee gobernanza antes de proponer cambios.
- La autonomia termina en cambios locales y commits locales; no hay push automatico.
- El runner usa acciones tipadas, no shell libre generado por IA.
- Los registros se guardan en una base separada `oneepis_agent` cuando `AGENT_DATABASE_URL` esta configurado.

## Comandos

```bash
npm install
npm run dev
npm run check
npm run agent -- autopilot --workspace "C:\\Users\\gdela\\OneDrive\\Documentos Importantes\\OneEpis"
npm run agent -- inspect "C:\\Users\\gdela\\OneDrive\\Documentos Importantes\\OneEpis"
npm run agent -- plan "C:\\Users\\gdela\\OneDrive\\Documentos Importantes\\OneEpis"
npm run agent -- run "C:\\Users\\gdela\\OneDrive\\Documentos Importantes\\OneEpis" --max-cycles 1
```

`autopilot` hace el flujo completo:

1. Crea o reutiliza el workspace local.
2. Clona `https://github.com/gabriel2320/oneepis.git` si OneEpis no existe.
3. Si el repo esta limpio, ejecuta `git fetch --prune origin` y `git pull --ff-only origin main`.
4. Lee gobernanza y scripts del repo.
5. Consulta Ollama local para microplan si esta disponible.
6. Selecciona un gate local seguro, por ejemplo `npm run check:size`, `npm run check:api` o `npm run check`.
7. Ejecuta el gate con proceso tipado, sin shell libre y sin push.

## Configuracion

```bash
OLLAMA_BASE_URL=http://localhost:11434
AGENT_DATABASE_URL=postgresql://postgres:postgres@localhost:5433/oneepis_agent
AGENT_PRIMARY_CODE_MODEL=qwen2.5-coder:14b
AGENT_GOVERNANCE_MODEL=qwen3:8b
AGENT_MAX_CYCLES=3
```

PostgreSQL de desarrollo separado:

```bash
docker compose -f infra/docker-compose.dev.yml up -d
```

URL local:

```bash
AGENT_DATABASE_URL=postgresql://oneepis_agent:oneepis_agent@localhost:5444/oneepis_agent
```

## Estado v0.2

- Inspeccion de repo objetivo.
- Deteccion OneEpis por `AGENTS.md` + `docs/GOVERNANCE.md`.
- Estado Ollama y modelos por politica.
- Plan de microciclo gobernado.
- Bitacora PostgreSQL opcional.
- Autopilot local controlado para clonar/actualizar OneEpis y ejecutar el siguiente gate local.

La generacion y aplicacion de patches reales sigue bloqueada hasta que exista un arnes de tests especifico para cambios automaticos. Esta version ejecuta trabajo local verificable, no PRs remotos ni push automatico.
