// Re-exporta o conteudo de _module_star.ts como namespace agregado.
// `import { starNs } from "./_module_ns_reexport"` -> starNs.one() etc.

export * as starNs from "./_module_star";
