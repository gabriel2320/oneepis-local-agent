# Gobierno del Laboratorio

OneEpis Local Agent es un laboratorio externo. Puede trabajar sobre OneEpis, pero no es parte de OneEpis.

## Reglas duras

- No escribir dentro de un repo objetivo sin crear rama local segura.
- No hacer push automatico.
- No ejecutar shell libre.
- No editar reglas de gobernanza para permitir su propio cambio.
- No persistir secretos, PHI ni identificadores reales en bitacora.
- No continuar indefinidamente: maximo 3 ciclos por corrida.

## Estados del ciclo

```text
preflight -> governance_read -> repo_audit -> micro_plan -> patch_draft
-> safety_review -> apply_patch -> gate_run -> result_record
-> lesson_record -> stop_or_next
```

## OneEpis como repo objetivo

Si el repo objetivo es OneEpis, el agente debe leer `AGENTS.md` y `docs/GOVERNANCE.md` antes de cualquier plan.

Debe preferir fixes pequeños, dieta, tests y contratos minimos. No debe abrir dashboards, IA protagonista, labs pegados al core, receta, firma, RAG ni pantallas clinicas nuevas sin plan explicito.

