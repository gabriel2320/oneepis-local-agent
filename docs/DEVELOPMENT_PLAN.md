# OneEpis Local Agent Development Plan

## Direction

OneEpis Local Agent is a separate local development assistant for OneEpis. It uses Ollama only, reads governance before proposing work, and keeps autonomy bounded by typed actions, local branches, explicit gates, and human confirmation.

The app is not part of the OneEpis clinical repo and must not move clinical truth, patient data, or product governance into the agent.

## Releases

### v0.1 Stabilization

- Keep the Tauri/React/Rust scaffold green with `npm run check`.
- Publish a dedicated GitHub repository for the agent.
- Keep the desktop shortcut pointed at `npm run dev` in this repo.
- Document Ollama, PostgreSQL, and CLI setup.

### v0.2 Planning Depth

- Generate structured `PatchDraft` records without writing target files.
- Review drafts with deterministic checks: risk, diff size, paths, gates, and blocked state.
- List persisted runs from the optional `oneepis_agent` database.
- Run declared package gates through typed commands only.

### v0.3 Controlled Execution

- Apply patches only when the target repo is Git, clean, on a safe local branch, and the review token matches.
- Use `git apply --check` before `git apply`.
- Record human decisions and gate results.
- Block red-risk work and dirty target repos.

### v0.4 OneEpis Adapter

- Deepen detection of `AGENTS.md`, `docs/GOVERNANCE.md`, `docs/SCREEN_TREE.md`, OpenAPI, and official gates.
- Prefer patient, chart, paper, API, PostgreSQL, audit, permissions, and OpenAPI work.
- Keep dashboards, broad RAG, clinical signature, prescriptions, and AI-protagonist flows blocked unless a specific plan exists.

### v0.5 Local Distribution

- Add repeatable Tauri packaging.
- Publish GitHub releases.
- Document supported Ollama models and local hardware expectations.

## Public Contracts

- `MicroPlan`: objective, recommended gate, risk level, touched surfaces, required gates, steps, warnings, blocked state, and model used.
- `PatchDraft`: summary, rationale, proposed files, unified diff, risks, gates, blocked state, plan, and creation metadata.
- `PatchReview`: deterministic checks, blocks, approval status, and confirmation token.
- `GateResult`: command, status, exit code, duration, sanitized stdout, and sanitized stderr.

## Gates

Every PR must run:

```bash
npm run check
```

Target repo gates are only run when they are declared in the target `package.json` as `check`, `check:*`, `test`, or `build`.
