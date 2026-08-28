import { type ProvenanceGraph, type ProvenanceNode } from "@cinematic/domain";
import { useEffect, useState } from "react";
import { getProvenanceGraph } from "./api";

interface ProvPanelProps {
  projectRootPath: string;
  targetKind: string;
  targetId: string;
  onNavigate?: (kind: string, id: string) => void;
}

export function ProvenancePanel({
  projectRootPath,
  targetKind,
  targetId,
  onNavigate,
}: ProvPanelProps) {
  const [graph, setGraph] = useState<ProvenanceGraph | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [expandedNode, setExpandedNode] = useState<string | null>(targetId);

  useEffect(() => {
    const load = async () => {
      try {
        setIsLoading(true);
        setError(null);
        const result = await getProvenanceGraph(
          projectRootPath,
          targetKind,
          targetId
        );
        setGraph(result);
      } catch (err) {
        setError(
          err instanceof Error ? err.message : "Failed to load provenance"
        );
      } finally {
        setIsLoading(false);
      }
    };

    load();
  }, [projectRootPath, targetKind, targetId]);

  if (isLoading) {
    return (
      <div className="flex items-center justify-center p-8">
        <div className="animate-spin">⟳</div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="p-4 bg-red-50 border border-red-200 rounded text-red-700">
        <p className="font-semibold">Error loading provenance</p>
        <p className="text-sm">{error}</p>
      </div>
    );
  }

  if (!graph) {
    return (
      <div className="p-4 text-gray-500">
        No provenance data available for this item.
      </div>
    );
  }

  // Build adjacency for rendering
  const nodeById = new Map(graph.nodes.map((n) => [n.id, n]));
  const edgesByFrom = new Map<string, string[]>();
  const edgesByTo = new Map<string, Array<{ from: string; relation: string }>>();

  graph.edges.forEach((edge) => {
    if (!edgesByFrom.has(edge.from)) {
      edgesByFrom.set(edge.from, []);
    }
    edgesByFrom.get(edge.from)!.push(edge.to);

    if (!edgesByTo.has(edge.to)) {
      edgesByTo.set(edge.to, []);
    }
    edgesByTo.get(edge.to)!.push({ from: edge.from, relation: edge.relation });
  });

  const renderNode = (nodeId: string, depth: number = 0): JSX.Element => {
    const node = nodeById.get(nodeId);
    if (!node) return <></>;

    const isExpanded = expandedNode === nodeId;
    const incoming = edgesByTo.get(nodeId) || [];
    const outgoing = edgesByFrom.get(nodeId) || [];

    return (
      <div
        key={nodeId}
        className="mb-2"
        style={{ marginLeft: `${depth * 20}px` }}
      >
        <div
          className="flex items-center gap-2 p-2 bg-gray-50 rounded border border-gray-200 hover:bg-gray-100 cursor-pointer group"
          onClick={() =>
            setExpandedNode(isExpanded ? null : nodeId)
          }
        >
          {(incoming.length > 0 || outgoing.length > 0) && (
            <button
              className="flex-shrink-0 text-gray-600"
              onClick={(e) => {
                e.stopPropagation();
                setExpandedNode(isExpanded ? null : nodeId);
              }}
            >
              {isExpanded ? "▼" : "▶"}
            </button>
          )}
          {(incoming.length === 0 && outgoing.length === 0) && (
            <div className="w-4" />
          )}
          <div className="flex-1 min-w-0">
            <div className="text-sm font-semibold text-gray-900 truncate">
              {node.label}
            </div>
            <div className="text-xs text-gray-600 flex gap-2">
              <span className="bg-blue-100 text-blue-800 px-2 py-0.5 rounded-full">
                {node.kind}
              </span>
              {node.timestamp && (
                <span className="text-gray-500">
                  {new Date(node.timestamp).toLocaleDateString()}
                </span>
              )}
            </div>
          </div>
          {onNavigate && (
            <button
              className="opacity-0 group-hover:opacity-100 px-2 py-1 text-xs bg-blue-100 text-blue-700 rounded hover:bg-blue-200 transition-opacity"
              onClick={(e) => {
                e.stopPropagation();
                onNavigate(node.kind, node.id);
              }}
            >
              View
            </button>
          )}
        </div>

        {isExpanded && incoming.length > 0 && (
          <div className="mt-2 pl-6 border-l border-gray-300">
            <div className="text-xs font-semibold text-gray-600 mb-2">
              Dependencies
            </div>
            {incoming.map((edge) => (
              <div key={`${edge.from}-${nodeId}`} className="mb-2">
                <div className="text-xs text-gray-500 mb-1">
                  ← {edge.relation}
                </div>
                {renderNode(edge.from, 1)}
              </div>
            ))}
          </div>
        )}

        {isExpanded && outgoing.length > 0 && (
          <div className="mt-2 pl-6 border-l border-gray-300">
            <div className="text-xs font-semibold text-gray-600 mb-2">
              Dependents
            </div>
            {outgoing.map((toNodeId) => {
              const edgeLabel = graph.edges
                .filter((e) => e.from === nodeId && e.to === toNodeId)
                .map((e) => e.relation)
                .join(", ");
              return (
                <div key={`${nodeId}-${toNodeId}`} className="mb-2">
                  <div className="text-xs text-gray-500 mb-1">
                    {edgeLabel} →
                  </div>
                  {renderNode(toNodeId, 1)}
                </div>
              );
            })}
          </div>
        )}
      </div>
    );
  };

  return (
    <div className="p-4 bg-white border rounded">
      <h3 className="text-lg font-semibold mb-4">Provenance</h3>
      <div className="space-y-2">
        {renderNode(targetId)}
      </div>
      <div className="mt-6 pt-4 border-t text-xs text-gray-600">
        <p>
          Showing {graph.nodes.length} node{graph.nodes.length !== 1 ? "s" : ""} and{" "}
          {graph.edges.length} relationship{graph.edges.length !== 1 ? "s" : ""}
        </p>
      </div>
    </div>
  );
}
