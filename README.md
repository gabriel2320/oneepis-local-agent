# OneEpis Local Agent

Mini Cursor local gobernado para microciclos de desarrollo con Ollama.

Este repo es una herramienta externa. No es OneEpis, no vive dentro de OneEpis y no reemplaza la gobernanza del repo objetivo.

## Principios

- Ollama local es el unico proveedor IA.
- El agente lee gobernanza antes de proponer cambios.
- La autonomia termina en cambios locales y commits locales; no hay push automatico.
- El runner usa acciones tipadas, no shell libre generado por IA.
- Los registros se guardan en una base separada `oneepis_agent` cuando `AGENT_DATABASE_URL` esta configurado.

## Comandos

```bash
npm install
npm run dev
npm run check
npm run agent -- inspect "C:\\Users\\gdela\\OneDrive\\Documentos Importantes\\OneEpis"
npm run agent -- plan "C:\\Users\\gdela\\OneDrive\\Documentos Importantes\\OneEpis"
npm run agent -- run "C:\\Users\\gdela\\OneDrive\\Documentos Importantes\\OneEpis" --max-cycles 1
```

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

## Estado v0.1

- Inspeccion de repo objetivo.
- Deteccion OneEpis por `AGENTS.md` + `docs/GOVERNANCE.md`.
- Estado Ollama y modelos por politica.
- Plan de microciclo gobernado.
- Bitacora PostgreSQL opcional.
- Runner dry-run con maquina de estados cerrada.

La ejecucion con patches reales queda bloqueada hasta que los tests de v0.3 esten completos.
