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
      headline: "El borrador esta revisado y listo para decision humana.",
      body: `Propuse una lista acotada de cambios sobre ${input.draft.files.join(", ")} y todas las revisiones automaticas pasaron.`,
      nextAction: "Ejecutar la prueba recomendada antes de aplicar cualquier cambio.",
      guardrail: "La escritura real requiere proyecto limpio, rama segura agent/<objetivo> y confirmacion humana.",
      tone: "success",
      checklist: gatesChecklist(input.draft.gates),
      power: "Puedo preparar la aplicacion controlada, pero no publico cambios automaticamente.",
    };
  }

  if (input.draft?.blocked || input.review?.approved === false) {
    return {
      headline: "El borrador necesita ajuste antes de avanzar.",
      body: "La lista de cambios, las pruebas o el nivel de riesgo no cumplen una condicion de seguridad.",
      nextAction: input.review?.blocks[0] ?? "Reducir el alcance y regenerar el borrador.",
      guardrail: "OneEpis solo acepta procesos pequenos con cambios limitados, pruebas oficiales y aprendizaje comprobable.",
      tone: "warning",
      checklist: input.review?.blocks.length ? input.review.blocks : ["Revisar riesgo", "Revisar pruebas", "Regenerar borrador"],
      power: "Puedo volver a proponer un borrador mas pequeno.",
    };
  }

  if (input.gateResult) {
    const passed = input.gateResult.status === "passed";
    return {
      headline: passed ? "La prueba termino correctamente." : "La prueba encontro un problema.",
      body: `${input.gateResult.command} termino con estado ${input.gateResult.status}.`,
      nextAction: passed ? "Registrar el resultado y decidir si el proceso corto queda cerrado." : "Leer la salida de la prueba y reducir el cambio.",
      guardrail: "Una prueba fallida detiene el ciclo antes de aplicar o ampliar alcance.",
      tone: passed ? "success" : "danger",
      checklist: [input.gateResult.summary],
      power: "Puedo ejecutar pruebas declaradas por package.json, no comandos libres.",
    };
  }

  if (input.brief) {
    const proposed = input.brief.proposal?.status === "proposed";
    return {
      headline: proposed ? "El modelo local propuso un camino revisable." : "Las instrucciones locales estan listas para revision.",
      body: `${input.brief.contextFiles.length} entradas de contexto; modelo ${input.brief.modelUsed}.`,
      nextAction: proposed ? "Revisar propuesta y convertir una sola decision en borrador." : input.brief.nextActions[0] ?? "Pedir propuesta al modelo local.",
      guardrail: "Estas instrucciones no aplican cambios: solo orientan a Ollama y conservan pruebas, riesgos y parada.",
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
      nextAction: ready ? "Crear un borrador revisable con ese contexto acotado." : input.contextPack.warnings[0] ?? "Reducir el contexto antes de crear el borrador.",
      guardrail: "La lectura local es solo lectura, no contiene secretos conocidos y no permite aplicar cambios por si sola.",
      tone: ready ? "success" : "warning",
      checklist: input.contextPack.promptNotes,
      power: "Puedo preparar memoria de trabajo local para programar con Ollama sin salir de las reglas de OneEpis.",
    };
  }

  if (input.run) {
    const completed = input.run.status === "completed";
    return {
      headline: completed ? "El microproceso cerro una pasada segura." : "El microproceso quedo bloqueado.",
      body: `Revise reglas, proyecto, plan, borrador, seguridad y parada en modo ${input.run.mode}.`,
      nextAction: completed ? `Correr ${input.run.plan.recommendedGate} si aun no se ejecuto.` : "Resolver el bloqueo indicado en safety_review.",
      guardrail: "El ensayo sin cambios no escribe archivos del proyecto objetivo.",
      tone: completed ? "success" : "warning",
      checklist: input.run.steps.map((step) => `${step.state}: ${step.status}`),
      power: "Puedo repetir hasta 3 ciclos, siempre con parada explicita.",
    };
  }

  if (input.plan) {
    return {
      headline: input.plan.blocked ? "El plan esta bloqueado por las reglas de OneEpis." : "Tengo un plan pequeno y gobernado.",
      body: `Riesgo ${input.plan.riskLevel}; prueba recomendada ${input.plan.recommendedGate || "sin prueba"}.`,
      nextAction: input.plan.blocked ? "Reducir alcance antes de crear el borrador." : "Crear un borrador revisable sin escribir en OneEpis.",
      guardrail: "El agente prioriza paciente, ficha, papel, API, PostgreSQL, auditoria, permisos y OpenAPI.",
      tone: input.plan.blocked ? "warning" : "success",
      checklist: input.plan.steps,
      power: "Puedo convertir el plan en una lista de cambios revisable y seleccionar pruebas oficiales.",
    };
  }

  if (input.inspection) {
    return {
      headline: input.inspection.isOneEpis ? "Reconozco OneEpis y activo sus reglas." : "Estoy trabajando sobre un proyecto generico.",
      body: `${input.inspection.projectName} esta en rama ${input.inspection.currentBranch}.`,
      nextAction: "Crear un plan pequeno antes de cualquier borrador.",
      guardrail: input.inspection.isOneEpis
        ? "Leo AGENTS.md y docs/GOVERNANCE.md antes de proponer trabajo."
        : "Aplico reglas basicas de safety; no asumo doctrina OneEpis completa.",
      tone: "neutral",
      checklist: input.inspection.detectedRules,
      power: "Puedo revisar proyecto, modelos, pruebas e historial local.",
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
      power: "Puedo seguir con reglas internas sin IA para un plan conservador.",
    };
  }

  return {
    headline: "Estoy listo para inspeccionar.",
    body: "Indica el proyecto y el proceso corto. Primero revisare reglas, Git, modelos y pruebas declaradas.",
    nextAction: "Presiona Revisar proyecto.",
    guardrail: "No escribo en el proyecto objetivo durante revision, plan ni borrador.",
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
    ready_for_confirmation: "listo para confirmacion",
    ready_to_apply: "listo para aplicar cambios",
  };
  return labels[status] ?? status;
}

function busyNarrative(busy: string): AgentNarrative {
  const messages: Record<string, AgentNarrative> = {
    inspect: {
      headline: "Estoy inspeccionando el entorno.",
      body: "Leo Git, reglas de OneEpis, modelos Ollama, pruebas e historial local.",
      nextAction: "Esperar el resultado de la revision inicial.",
      guardrail: "Esta etapa no escribe en el proyecto objetivo.",
      tone: "neutral",
      checklist: ["Git", "reglas de OneEpis", "Ollama", "pruebas"],
      power: "Puedo detectar bloqueos antes de planificar.",
    },
    plan: {
      headline: "Estoy creando un microplan gobernado.",
      body: "Uso el modelo local disponible o reglas internas si Ollama no responde.",
      nextAction: "Revisar riesgo, zonas tocadas y prueba recomendada.",
      guardrail: "Solo propongo pasos pequenos y verificables.",
      tone: "neutral",
      checklist: ["riesgo", "zonas tocadas", "pruebas", "avisos"],
      power: "Puedo adaptar el plan a la doctrina OneEpis.",
    },
    contextPack: {
      headline: "Estoy preparando contexto local.",
      body: "Leo solo rutas del paquete de trabajo, omito secretos y limito bytes para el modelo local.",
      nextAction: "Revisar archivos incluidos, avisos y notas para el modelo.",
      guardrail: "Esta etapa no escribe en OneEpis ni sustituye la revision humana.",
      tone: "neutral",
      checklist: ["paquete de trabajo", "rutas seguras", "extractos sanitizados", "pruebas"],
      power: "Puedo darle al modelo local contexto suficiente sin abrir todo el proyecto.",
    },
    brief: {
      headline: "Estoy preparando instrucciones para el modelo local.",
      body: "Convierto paquete y contexto en una orden de trabajo gobernada para Ollama.",
      nextAction: "Revisar mensaje, formato de respuesta y propuesta si el modelo responde.",
      guardrail: "Estas instrucciones no escriben archivos ni sustituyen el borrador/revision.",
      tone: "neutral",
      checklist: ["contexto", "mensaje", "formato de respuesta", "pruebas"],
      power: "Puedo pedir una propuesta local estructurada sin dar permisos para aplicar cambios.",
    },
    decision: {
      headline: "Estoy cerrando una decision de implementacion.",
      body: "Tomo las instrucciones y la propuesta local para seleccionar una sola intencion revisable.",
      nextAction: "Revisar archivos, pruebas, bloqueos y si queda listo para borrador.",
      guardrail: "La decision no escribe archivos; solo autoriza preparar un borrador revisable.",
      tone: "neutral",
      checklist: ["propuesta local", "archivos", "pruebas", "bloqueos"],
      power: "Puedo transformar una sugerencia de Ollama en una decision pequena y auditable.",
    },
    draft: {
      headline: "Estoy preparando un borrador de cambios.",
      body: "Genero una lista revisable de cambios y la paso por revisiones automaticas.",
      nextAction: "Revisar aprobacion y pruebas requeridas.",
      guardrail: "Esta etapa no escribe archivos del proyecto objetivo.",
      tone: "neutral",
      checklist: ["lista de cambios", "rutas", "pruebas", "riesgo"],
      power: "Puedo construir un borrador listo para decision humana.",
    },
    run: {
      headline: "Estoy ejecutando un ensayo sin cambios.",
      body: "Paso por la maquina de estados sin aplicar cambios.",
      nextAction: "Leer el resultado y cerrar el ciclo.",
      guardrail: "El ensayo se detiene antes de escritura real.",
      tone: "neutral",
      checklist: ["preflight", "safety_review", "result_record"],
      power: "Puedo cerrar una pasada con lecciones registrables.",
    },
    gate: {
      headline: "Estoy ejecutando una prueba declarada.",
      body: "Solo uso comandos permitidos desde package.json.",
      nextAction: "Esperar salida segura.",
      guardrail: "No ejecuto shell libre generado por IA.",
      tone: "neutral",
      checklist: ["script declarado", "salida sanitizada", "resultado"],
      power: "Puedo validar el proceso corto con la prueba mas pequena.",
    },
    microprocess: {
      headline: "Estoy corriendo un microproceso cerrado.",
      body: "Inspecciono, planifico, genero borrador, hago ensayo sin cambios, ejecuto prueba y paro.",
      nextAction: "Esperar la parada segura.",
      guardrail: "Un bloqueo corta el ciclo en safety_review.",
      tone: "neutral",
      checklist: ["proyecto", "plan", "borrador", "ensayo sin cambios", "prueba"],
      power: "Puedo coordinar el ciclo completo dentro de las reglas de OneEpis.",
    },
  };
  return messages[busy] ?? messages.inspect;
}

function blockerAction(blocker: string) {
  const text = blocker.toLowerCase();
  if (text.includes("worktree sucio")) return "Revisar cambios pendientes y dejar el proyecto limpio antes de aplicar cambios.";
  if (text.includes("ollama")) return "Iniciar Ollama o aceptar plan conservador con reglas internas sin IA.";
  if (text.includes("modelos")) return "Instalar el modelo faltante o ajustar AGENT_*_MODEL a un modelo local existente.";
  if (text.includes("no es git")) return "Elegir una carpeta que sea repo Git.";
  if (text.includes("governance")) return "Restaurar o ubicar docs/GOVERNANCE.md antes de continuar.";
  return "Resolver el bloqueo mostrado y volver a inspeccionar.";
}

function gatesChecklist(gates: string[]) {
  if (gates.length === 0) return ["Definir una prueba antes de aplicar"];
  return gates.map((gate) => `Prueba requerida: ${gate}`);
}
