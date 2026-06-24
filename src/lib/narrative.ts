import type {
  AgentRun,
  DevelopmentBrief,
  GateResult,
  DevelopmentContextPack,
  MicroPlan,
  OllamaStatus,
  PatchDraft,
  PatchReview,
  RepoInspection,
} from "./types";

export type NarrativeTone = "neutral" | "success" | "warning" | "danger";

export type AgentNarrative = {
  headline: string;
  body: string;
  nextAction: string;
  guardrail: string;
  tone: NarrativeTone;
  checklist: string[];
  power: string;
};

export type NarrativeInput = {
  inspection: RepoInspection | null;
  ollama: OllamaStatus | null;
  plan: MicroPlan | null;
  contextPack: DevelopmentContextPack | null;
  brief: DevelopmentBrief | null;
  draft: PatchDraft | null;
  review: PatchReview | null;
  gateResult: GateResult | null;
  run: AgentRun | null;
  blockers: string[];
  busy: string | null;
};

export function buildAgentNarrative(input: NarrativeInput): AgentNarrative {
  const blocker = input.blockers[0];
  if (input.busy) return busyNarrative(input.busy);

  if (blocker) {
    return {
      headline: "Estoy detenido por una condicion de seguridad.",
      body: "No sigo hacia cambios reales mientras exista este bloqueo. Primero hay que resolverlo y volver a inspeccionar.",
      nextAction: blockerAction(blocker),
      guardrail: blocker,
      tone: "warning",
      checklist: ["Resolver el bloqueo", "Inspeccionar de nuevo", "Generar un microplan pequeno"],
      power: "Puedo diagnosticar y planificar, pero no aplicar cambios.",
    };
  }

  if (input.review?.approved && input.draft) {
    return {
      headline: "El PatchDraft esta revisado y listo para decision humana.",
      body: `Propuse un diff acotado sobre ${input.draft.files.join(", ")} y todos los checks deterministas pasaron.`,
      nextAction: "Ejecutar el gate recomendado antes de cualquier apply.",
      guardrail: "La escritura real requiere repo limpio, rama segura agent/<objetivo> y token APPLY.",
      tone: "success",
      checklist: gatesChecklist(input.draft.gates),
      power: "Puedo preparar el apply controlado, pero no hago push automatico.",
    };
  }

  if (input.draft?.blocked || input.review?.approved === false) {
    return {
      headline: "El borrador necesita ajuste antes de avanzar.",
      body: "El diff, los gates o el nivel de riesgo no cumplen una condicion del safety kernel.",
      nextAction: input.review?.blocks[0] ?? "Reducir el alcance y regenerar PatchDraft.",
      guardrail: "OneEpis solo acepta microciclos con diff pequeno, gates oficiales y aprendizaje ejecutable.",
      tone: "warning",
      checklist: input.review?.blocks.length ? input.review.blocks : ["Revisar riesgo", "Revisar gates", "Regenerar draft"],
      power: "Puedo volver a proponer un draft mas pequeno.",
    };
  }

  if (input.gateResult) {
    const passed = input.gateResult.status === "passed";
    return {
      headline: passed ? "El gate termino correctamente." : "El gate encontro un problema.",
      body: `${input.gateResult.command} termino con estado ${input.gateResult.status}.`,
      nextAction: passed ? "Registrar el resultado y decidir si el microciclo queda cerrado." : "Leer la salida del gate y reducir el cambio.",
      guardrail: "Un gate fallido detiene el ciclo antes de aplicar o ampliar alcance.",
      tone: passed ? "success" : "danger",
      checklist: [input.gateResult.summary],
      power: "Puedo ejecutar gates declarados por package.json, no comandos libres.",
    };
  }

  if (input.brief) {
    const proposed = input.brief.proposal?.status === "proposed";
    return {
      headline: proposed ? "El modelo local propuso un camino revisable." : "El brief local esta listo para revision.",
      body: `${input.brief.contextFiles.length} entradas de contexto; modelo ${input.brief.modelUsed}.`,
      nextAction: proposed ? "Revisar propuesta y convertir una sola decision en PatchDraft." : input.brief.nextActions[0] ?? "Pedir propuesta al modelo local.",
      guardrail: "El brief no aplica cambios: solo orienta a Ollama y conserva gates, riesgos y parada.",
      tone: input.brief.status === "blocked" ? "warning" : proposed ? "success" : "neutral",
      checklist: input.brief.responseContract,
      power: "Puedo convertir contexto gobernado en una orden de trabajo para IA local sin abrir shell ni escritura.",
    };
  }

  if (input.contextPack) {
    const ready = input.contextPack.status === "ready";
    return {
      headline: ready ? "El contexto local esta listo para el modelo." : "El contexto local necesita revision.",
      body: `${input.contextPack.files.length} entradas, ${input.contextPack.totalBytes}/${input.contextPack.maxBytes} bytes sanitizados.`,
      nextAction: ready ? "Crear PatchDraft revisable con ese contexto acotado." : input.contextPack.warnings[0] ?? "Reducir el contexto antes de crear PatchDraft.",
      guardrail: "El context pack es solo lectura, no contiene secretos conocidos y no habilita apply por si solo.",
      tone: ready ? "success" : "warning",
      checklist: input.contextPack.promptNotes,
      power: "Puedo preparar memoria de trabajo local para programar con Ollama sin salir de gobernanza.",
    };
  }

  if (input.run) {
    const completed = input.run.status === "completed";
    return {
      headline: completed ? "El microproceso cerro una pasada segura." : "El microproceso quedo bloqueado.",
      body: `Revise gobernanza, repo, plan, patch, safety y parada en modo ${input.run.mode}.`,
      nextAction: completed ? `Correr ${input.run.plan.recommendedGate} si aun no se ejecuto.` : "Resolver el bloqueo indicado en safety_review.",
      guardrail: "El dry-run no escribe archivos del repo objetivo.",
      tone: completed ? "success" : "warning",
      checklist: input.run.steps.map((step) => `${step.state}: ${step.status}`),
      power: "Puedo repetir hasta 3 ciclos, siempre con parada explicita.",
    };
  }

  if (input.plan) {
    return {
      headline: input.plan.blocked ? "El plan esta bloqueado por gobernanza." : "Tengo un microplan gobernado.",
      body: `Riesgo ${input.plan.riskLevel}; gate recomendado ${input.plan.recommendedGate || "sin_gate"}.`,
      nextAction: input.plan.blocked ? "Reducir alcance antes de crear PatchDraft." : "Crear PatchDraft revisable sin escribir en OneEpis.",
      guardrail: "El agente prioriza paciente, ficha, papel, API, PostgreSQL, auditoria, permisos y OpenAPI.",
      tone: input.plan.blocked ? "warning" : "success",
      checklist: input.plan.steps,
      power: "Puedo convertir el plan en diff revisable y seleccionar gates oficiales.",
    };
  }

  if (input.inspection) {
    return {
      headline: input.inspection.isOneEpis ? "Reconozco OneEpis y activo su adaptador." : "Estoy trabajando sobre un repo generico.",
      body: `${input.inspection.projectName} esta en rama ${input.inspection.currentBranch}.`,
      nextAction: "Crear un microplan antes de cualquier PatchDraft.",
      guardrail: input.inspection.isOneEpis
        ? "Leo AGENTS.md y docs/GOVERNANCE.md antes de proponer trabajo."
        : "Aplico reglas basicas de safety; no asumo doctrina OneEpis completa.",
      tone: "neutral",
      checklist: input.inspection.detectedRules,
      power: "Puedo inspeccionar repo, modelos, gates y bitacora local.",
    };
  }

  if (input.ollama && !input.ollama.available) {
    return {
      headline: "Ollama no esta disponible.",
      body: input.ollama.message,
      nextAction: "Iniciar Ollama y volver a inspeccionar.",
      guardrail: "Sin Ollama puedo usar reglas locales, pero no pedir plan al modelo.",
      tone: "warning",
      checklist: input.ollama.missingPolicyModels,
      power: "Puedo seguir con local_rules para un plan conservador.",
    };
  }

  return {
    headline: "Estoy listo para inspeccionar.",
    body: "Indica el repo y el microciclo. Primero revisare gobernanza, Git, modelos y gates declarados.",
    nextAction: "Presiona Inspeccionar.",
    guardrail: "No escribo en el repo objetivo durante inspeccion, plan ni PatchDraft v0.2.",
    tone: "neutral",
    checklist: ["preflight", "governance_read", "repo_audit", "micro_plan"],
    power: "Puedo iniciar un ciclo cerrado y detenerme con resultado verificable.",
  };
}

export function explainStatus(status: string) {
  const labels: Record<string, string> = {
    completed: "completo",
    pending: "pendiente",
    running: "en curso",
    blocked: "bloqueado",
    failed: "fallo",
    skipped: "omitido",
    passed: "paso",
    approved: "aprobado",
  };
  return labels[status] ?? status;
}

function busyNarrative(busy: string): AgentNarrative {
  const messages: Record<string, AgentNarrative> = {
    inspect: {
      headline: "Estoy inspeccionando el entorno.",
      body: "Leo Git, gobernanza, modelos Ollama, gates y bitacora local.",
      nextAction: "Esperar el resultado de preflight.",
      guardrail: "Esta etapa no escribe en el repo objetivo.",
      tone: "neutral",
      checklist: ["Git", "gobernanza", "Ollama", "gates"],
      power: "Puedo detectar bloqueos antes de planificar.",
    },
    plan: {
      headline: "Estoy creando un microplan gobernado.",
      body: "Uso el modelo de gobernanza disponible o reglas locales si Ollama no responde.",
      nextAction: "Revisar riesgo, superficies y gate recomendado.",
      guardrail: "Solo propongo pasos pequenos y verificables.",
      tone: "neutral",
      checklist: ["riesgo", "superficies", "gates", "warnings"],
      power: "Puedo adaptar el plan a la doctrina OneEpis.",
    },
    contextPack: {
      headline: "Estoy preparando contexto local.",
      body: "Leo solo rutas del paquete de trabajo, omito secretos y limito bytes para el modelo local.",
      nextAction: "Revisar archivos incluidos, warnings y notas de prompt.",
      guardrail: "Esta etapa no escribe en OneEpis ni sustituye la revision humana.",
      tone: "neutral",
      checklist: ["paquete de trabajo", "rutas seguras", "extractos sanitizados", "gates"],
      power: "Puedo darle al modelo local contexto suficiente sin abrir todo el repo.",
    },
    brief: {
      headline: "Estoy preparando el brief para el modelo local.",
      body: "Convierto paquete y contexto en una orden de trabajo gobernada para Ollama.",
      nextAction: "Revisar prompt, contrato de respuesta y propuesta si el modelo responde.",
      guardrail: "El brief no escribe archivos ni sustituye PatchDraft/revision.",
      tone: "neutral",
      checklist: ["contexto", "prompt", "contrato JSON", "gates"],
      power: "Puedo pedir una propuesta local estructurada sin dar permisos de apply.",
    },
    draft: {
      headline: "Estoy preparando un PatchDraft.",
      body: "Genero un diff revisable y lo paso por checks deterministas.",
      nextAction: "Revisar aprobacion y gates requeridos.",
      guardrail: "v0.2 no escribe archivos del repo objetivo.",
      tone: "neutral",
      checklist: ["diff", "paths", "gates", "riesgo"],
      power: "Puedo construir un borrador listo para decision humana.",
    },
    run: {
      headline: "Estoy ejecutando un dry-run.",
      body: "Paso por la maquina de estados sin aplicar cambios.",
      nextAction: "Leer el resultado y cerrar el ciclo.",
      guardrail: "El dry-run se detiene antes de escritura real.",
      tone: "neutral",
      checklist: ["preflight", "safety_review", "result_record"],
      power: "Puedo cerrar una pasada con lecciones registrables.",
    },
    gate: {
      headline: "Estoy ejecutando un gate declarado.",
      body: "Solo uso scripts permitidos desde package.json.",
      nextAction: "Esperar stdout/stderr sanitizado.",
      guardrail: "No ejecuto shell libre generado por IA.",
      tone: "neutral",
      checklist: ["script declarado", "salida sanitizada", "resultado"],
      power: "Puedo validar el microciclo con el gate mas pequeno.",
    },
    microprocess: {
      headline: "Estoy corriendo un microproceso cerrado.",
      body: "Inspecciono, planifico, genero PatchDraft, hago dry-run, ejecuto gate y paro.",
      nextAction: "Esperar la parada segura.",
      guardrail: "Un bloqueo corta el ciclo en safety_review.",
      tone: "neutral",
      checklist: ["repo", "plan", "patch", "dry-run", "gate"],
      power: "Puedo coordinar el ciclo completo dentro de gobernanza.",
    },
  };
  return messages[busy] ?? messages.inspect;
}

function blockerAction(blocker: string) {
  const text = blocker.toLowerCase();
  if (text.includes("worktree sucio")) return "Revisar cambios pendientes y dejar el repo limpio antes de apply.";
  if (text.includes("ollama")) return "Iniciar Ollama o aceptar plan conservador con local_rules.";
  if (text.includes("modelos")) return "Instalar el modelo faltante o ajustar AGENT_*_MODEL a un modelo local existente.";
  if (text.includes("no es git")) return "Elegir una carpeta que sea repo Git.";
  if (text.includes("governance")) return "Restaurar o ubicar docs/GOVERNANCE.md antes de continuar.";
  return "Resolver el bloqueo mostrado y volver a inspeccionar.";
}

function gatesChecklist(gates: string[]) {
  if (gates.length === 0) return ["Definir gate antes de aplicar"];
  return gates.map((gate) => `Gate requerido: ${gate}`);
}
