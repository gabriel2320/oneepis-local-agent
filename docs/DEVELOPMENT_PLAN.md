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
- Record dry-run states for package, context pack, local brief, microplan, PatchDraft, safety, result, lessons, and stop condition.
- Explain every state in natural Spanish: current action, reason, next action, and active governance guardrail.
- Keep help text operational and tied to cycle state, not marketing copy.
- Provide `DevelopmentReadiness` before planning: repo readiness, Ollama/model health, required gates, blockers, next actions, and suggested microcycles.
- Provide `DevelopmentWorkPackage` for a selected objective: files to inspect, implementation steps, test plan, acceptance criteria, stop conditions, gates, and branch strategy.
- Provide `DevelopmentContextPack` for the selected objective: bounded local snippets, directory summaries, sanitization warnings, prompt notes, and gates for Ollama-only programming.
- Provide `DevelopmentBrief` between context and patch: governed system/user prompts, JSON response contract, stop conditions, and optional local Ollama proposal.
- Provide `AgentRunReport` after a dry-run: Markdown for PR review, status verdict, checklist, warnings, next actions, gate recommendation, and lessons.

### v0.3 Controlled Execution

- Prevalidate controlled apply with `ApplyReadiness`: deterministic review, clean Git, safe branch, `git apply --check`, and human confirmation state.
- Apply patches only when the target repo is Git, clean, on a safe local branch, and the review token matches.
- Use `git apply --check` before `git apply`.
- Record human decisions and gate results.
- Block red-risk work and dirty target repos.

### v0.4 OneEpis Adapter

- Deepen detection of `AGENTS.md`, `docs/GOVERNANCE.md`, `docs/SCREEN_TREE.md`, OpenAPI, and official gates.
- Prefer patient, chart, paper, API, PostgreSQL, audit, permissions, and OpenAPI work.
- Keep dashboards, broad RAG, clinical signature, prescriptions, and AI-protagonist flows blocked unless a specific plan exists.
- Show governed autonomy explicitly: inspect, plan, draft, review, run gates, and prepare controlled apply only when OneEpis rules allow it.
- Treat OneEpis warnings as visible guidance and hard blocks as stop conditions with concrete repair actions.

### v0.4.1 Usability And Clarity

- Prevent long governance text, Windows paths, model names, tokens, diffs, and gate output from overflowing cards.
- Translate technical states such as `completed`, `blocked`, `passed`, and `failed` into Spanish labels.
- Keep the first screen operational with repo, objective, cycle controls, blockers, and natural-language agent status.
- Show context packs with wrapping cards, explicit omissions, byte budgets, and notes that tell the local model how to use the context.
- Show local briefs with the exact prompts, expected response schema, proposal summary, risks, gates, and no-apply guardrail.

### v0.5 Local Distribution

- Add repeatable Tauri packaging.
- Publish GitHub releases.
- Document supported Ollama models and local hardware expectations.

## Public Contracts

- `MicroPlan`: objective, recommended gate, risk level, touched surfaces, required gates, steps, warnings, blocked state, and model used.
- `PatchDraft`: summary, rationale, proposed files, unified diff, risks, gates, blocked state, plan, and creation metadata.
- `PatchReview`: deterministic checks, blocks, approval status, and confirmation token.
- `ApplyReadiness`: read-only controlled-apply preflight with target branch, current branch, token, checks, blocks, status, and next actions.
- `GateResult`: command, status, exit code, duration, sanitized stdout, and sanitized stderr.
- `AgentNarrative`: frontend-only explanation of what the agent is doing, why, next action, guardrail, visible power, and checklist.
- `DevelopmentReadiness`: Spanish readiness report for local OneEpis programming with checks, blockers, warnings, next actions, required gates, model summary, and suggested microcycles.
- `DevelopmentWorkPackage`: executable planning contract for one local programming microcycle, including tests and acceptance criteria.
- `DevelopmentContextPack`: read-only local context contract with proposed files, sanitized excerpts, directory summaries, skipped/missing paths, prompt notes, gates, and byte budget.
- `DevelopmentBrief`: read-only local model work order with prompts, response contract, context files, next actions, stop conditions, and optional `LocalModelProposal`.
- `LocalModelProposal`: Ollama-only structured suggestion with summary, files to change, implementation notes, risks, gates, raw sanitized response, and model used.
- `AgentRunReport`: Spanish/Markdown review artifact for closed microprocesses, including run id, verdict, objective, branch, model, recommended gate, checklist, warnings, next actions, and lessons.
- Proposals that mention files outside the governed context or gates outside the package become `needs_review`, not an approved path to PatchDraft.

## Governed Power

The agent may become more useful without becoming unbounded. Extra power must follow this ladder:

1. Inspect local repo, Git, governance, Ollama, gates, and history.
2. Produce a readiness diagnosis with blockers, warnings, gates, model health, and suggested microcycles.
3. Produce a work package with files, steps, tests, acceptance criteria, and stop conditions.
4. Produce a context pack with bounded, sanitized local snippets for Ollama.
5. Produce a brief/proposal for the local model without writing target files.
6. Produce a microplan with risk, surfaces, gates, and warnings.
7. Produce a `PatchDraft` without writing target files.
8. Review the draft with deterministic safety checks.
9. Prevalidate controlled apply without writing: clean Git, safe branch, token state, and `git apply --check`.
10. Run only declared gates from `package.json`.
11. Apply only in v0.3+ with clean Git, safe branch, approved review, confirmation token, and no red risk.
12. Never push automatically.

## Gates

Every PR must run:

```bash
npm run check
```

Target repo gates are only run when they are declared in the target `package.json` as `check`, `check:*`, `test`, or `build`.

## PR And Microprocess Discipline

- Every code change to this agent should land through a branch and pull request.
- Every PR should document the closed microprocess it exercised: objective, preflight result, PatchDraft/review state, gate result, and stop condition.
- Prefer attaching or pasting the generated `AgentRunReport` when the change updates agent behavior.
- When a PR changes agent behavior, include the context-pack result or explain why no context pack was needed.
- If the local model is invoked, include the brief/proposal status and the model used.
- A blocked target repo is still a valid microprocess result when the block is explained and no target files are changed.
