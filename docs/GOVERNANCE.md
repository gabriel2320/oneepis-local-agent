# Gobierno del Laboratorio

OneEpis Local Agent es un laboratorio externo. Puede trabajar sobre OneEpis, pero no es parte de OneEpis.

## Reglas duras

- No escribir dentro de un repo objetivo sin crear rama local segura.
- No hacer push automatico.
- No ejecutar shell libre.
- No editar reglas de gobernanza para permitir su propio cambio.
- No persistir secretos, PHI ni identificadores reales en bitacora.
- No continuar indefinidamente: maximo 3 ciclos por corrida.
- No ocultar bloqueos: toda parada debe explicar accion concreta en lenguaje natural.

## Estados del ciclo

```text
preflight -> governance_read -> repo_audit -> evolution_plan -> work_package
-> context_pack -> development_brief -> implementation_decision -> micro_plan -> patch_draft
-> safety_review -> apply_patch -> gate_run -> result_record
-> lesson_record -> stop_or_next
```

## OneEpis Como Repo Objetivo

Si el repo objetivo es OneEpis, el agente debe leer `AGENTS.md` y `docs/GOVERNANCE.md` antes de cualquier plan.

Debe preferir fixes pequenos, dieta, tests y contratos minimos. No debe abrir dashboards, IA protagonista, labs pegados al core, receta, firma, RAG ni pantallas clinicas nuevas sin plan explicito.

## Autonomia Gobernada

El agente local puede actuar con mas poder solo dentro de estos limites:

- inspeccionar repo, Git, gobernanza, Ollama, gates y bitacora;
- diagnosticar preparacion local con bloqueos, warnings, gates requeridos, salud de modelos y microciclos sugeridos;
- puntuar candidatos de evolucion supervisada con `EvolutionPlan`, rechazar banderas prohibidas y elegir un solo microproceso o detenerse;
- crear paquete de trabajo con archivos a inspeccionar, pasos, plan de pruebas, criterios de aceptacion y condiciones de parada;
- crear context pack local de solo lectura con extractos sanitizados, limites de bytes, omisiones explicitas y notas para Ollama;
- crear brief local de solo lectura con prompts, contrato JSON, propuesta Ollama opcional y condiciones de parada;
- convertir una propuesta local en `ImplementationDecision` de solo lectura, con archivos, gates, bloqueos y aceptacion;
- producir microplan con riesgo, superficies, gates y warnings;
- producir `PatchDraft` revisable sin escritura real;
- revisar drafts con checks deterministas;
- prevalidar apply con `ApplyReadiness` sin escribir archivos ni cambiar de rama;
- ejecutar un gate declarado por `package.json`;
- preparar apply controlado solo desde v0.3, con repo limpio, rama segura, token humano y riesgo no rojo.

No puede hacer push automatico, ampliar alcance para destrabar su propio plan ni ejecutar comandos no tipados.

El context pack no autoriza escritura: si hay repo sucio, ruta sensible, archivo enorme, PHI probable o contexto insuficiente, debe mostrar warning y pedir un microciclo mas pequeno.

El brief/propuesta del modelo tampoco autoriza escritura: una sugerencia de Ollama debe convertirse despues en `PatchDraft`, pasar revision deterministica y gate declarado. Si Ollama propone rutas fuera del contexto gobernado o gates no declarados, la propuesta queda `needs_review`.

Si Ollama omite archivos pero la propuesta sigue siendo gobernada, el agente puede inferir hasta tres archivos seguros desde el contexto local ya cargado. Esa inferencia debe quedar visible como warning y no permite escritura por si sola.

## Lenguaje Natural

Toda pantalla operativa debe decir en espanol simple:

- que esta haciendo el agente;
- por que lo esta haciendo;
- cual es la siguiente accion;
- que baranda de gobernanza esta activa;
- que poder local puede usar ahora.
