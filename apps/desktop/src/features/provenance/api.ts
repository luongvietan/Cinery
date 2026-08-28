import type { ProvenanceGraph } from "@cinematic/domain";
import { invokeCommand } from "../../lib/tauri";

export function getProvenanceGraph(
  projectRootPath: string,
  targetKind: string,
  targetId: string
): Promise<ProvenanceGraph> {
  return invokeCommand("get_provenance_graph", {
    projectRootPath,
    targetKind,
    targetId,
  });
}
