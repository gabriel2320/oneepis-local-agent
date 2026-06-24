const replacements: Array<[RegExp, string]> = [
  [/\bpreflight\b/g, "revision inicial"],
  [/\bgovernance_read\b/g, "lectura de reglas"],
  [/\brepo_audit\b/g, "revision del proyecto"],
  [/\bmicro_plan\b/g, "plan pequeno"],
  [/\bcontext_pack\b/g, "contexto de trabajo"],
  [/\bimplementation_decision\b/g, "decision de trabajo"],
  [/\bpatch_draft\b/g, "borrador de cambios"],
  [/\bsafety_review\b/g, "revision de seguridad"],
  [/\bapply_patch\b/g, "aplicar cambios"],
  [/\bgate_run\b/g, "ejecutar prueba"],
  [/\bresult_record\b/g, "registro de resultado"],
  [/\blesson_record\b/g, "aprendizaje registrado"],
  [/\bstop_or_next\b/g, "cierre o siguiente paso"],
  [/\bPatchDraft\b/g, "borrador de cambios"],
  [/\bdiff\b/gi, "lista de cambios"],
  [/\bgates?\b/gi, "pruebas"],
  [/\bgate\b/gi, "prueba"],
  [/\bdry-run\b/gi, "ensayo sin cambios"],
  [/\bapply\b/gi, "aplicar cambios"],
  [/\brepo\b/gi, "proyecto"],
  [/\bworktree sucio\b/gi, "hay cambios pendientes"],
  [/\bgobernanza\b/gi, "reglas de OneEpis"],
  [/\bgovernance\b/gi, "reglas de OneEpis"],
  [/\blocal_rules\b/g, "reglas internas sin IA"],
  [/\bsafety kernel\b/gi, "revisión de seguridad"],
  [/\bbrief\b/gi, "instrucciones para el modelo local"],
  [/\bcontext pack\b/gi, "contexto de trabajo"],
  [/\bmicrociclo\b/gi, "paso pequeño"],
  [/\bmicroproceso\b/gi, "proceso corto"],
  [/\bscore\b/gi, "puntaje"],
  [/\bwarnings?\b/gi, "avisos"],
  [/\brun\b/gi, "ejecución"],
  [/\bruns\b/gi, "ejecuciones"],
];

export function plainText(value: string) {
  return replacements.reduce((text, [pattern, replacement]) => text.replace(pattern, replacement), value);
}

export function plainItems(items: string[]) {
  return items.map(plainText);
}
