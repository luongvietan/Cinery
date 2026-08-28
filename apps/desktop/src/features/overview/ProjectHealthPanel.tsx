import React, { useEffect, useState } from "react";
import type { ProjectHealthIssue } from "@cinematic/domain";
import { getProjectHealth } from "./healthApi";

interface Props {
  projectRootPath: string;
}

export const ProjectHealthPanel: React.FC<Props> = ({ projectRootPath }) => {
  const [issues, setIssues] = useState<ProjectHealthIssue[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setIsLoading(true);
    setError(null);

    getProjectHealth(projectRootPath)
      .then((result) => {
        setIssues(result);
      })
      .catch((err) => {
        setError(
          err instanceof Error ? err.message : "Failed to scan project health."
        );
      })
      .finally(() => {
        setIsLoading(false);
      });
  }, [projectRootPath]);

  if (isLoading) {
    return (
      <div className="space-y-2 p-4">
        <div className="h-4 bg-gray-200 rounded animate-pulse"></div>
        <div className="h-4 bg-gray-200 rounded animate-pulse"></div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="p-4 bg-red-50 border border-red-200 rounded">
        <p className="text-red-700 text-sm">Health scan error: {error}</p>
      </div>
    );
  }

  const errorCount = issues.filter((i) => i.severity === "error").length;
  const warningCount = issues.filter((i) => i.severity === "warning").length;
  const infoCount = issues.filter((i) => i.severity === "info").length;
  const fatalCount = issues.filter((i) => i.severity === "fatal").length;

  const hasIssues = issues.length > 0;

  return (
    <div className="space-y-4 p-4 border rounded-lg bg-white">
      <div className="flex items-center justify-between">
        <h3 className="font-semibold text-gray-900">Project Health</h3>
        {!hasIssues ? (
          <span className="text-sm text-green-600 font-medium">✓ Clean</span>
        ) : (
          <span className="text-sm text-amber-600 font-medium">
            {fatalCount > 0 ? `${fatalCount} fatal` : ""}
            {errorCount > 0 ? `, ${errorCount} error${errorCount > 1 ? "s" : ""}` : ""}
            {warningCount > 0 ? `, ${warningCount} warning${warningCount > 1 ? "s" : ""}` : ""}
            {infoCount > 0 ? `, ${infoCount} info` : ""}
          </span>
        )}
      </div>

      {hasIssues ? (
        <div className="space-y-2">
          {issues.map((issue) => (
            <div
              key={issue.code + (issue.entityId || "global")}
              className={`p-3 rounded border-l-4 ${getSeverityStyles(issue.severity)}`}
            >
              <div className="flex items-start justify-between">
                <div className="flex-1">
                  <p className="font-medium text-sm">{issue.code}</p>
                  <p className="text-sm text-gray-700 mt-1">{issue.message}</p>
                  {issue.remediation && (
                    <p className="text-xs text-gray-600 mt-2 italic">
                      {issue.remediation}
                    </p>
                  )}
                  {issue.entityId && (
                    <p className="text-xs text-gray-500 mt-1">
                      Entity: {issue.entityId}
                    </p>
                  )}
                </div>
                <span
                  className={`ml-2 px-2 py-1 text-xs font-medium rounded whitespace-nowrap ${getSeverityBadgeStyles(issue.severity)}`}
                >
                  {issue.severity}
                </span>
              </div>
            </div>
          ))}
        </div>
      ) : (
        <p className="text-sm text-gray-600">
          No integrity issues detected. Your project is in good health.
        </p>
      )}
    </div>
  );
};

function getSeverityStyles(
  severity: ProjectHealthIssue["severity"]
): string {
  switch (severity) {
    case "fatal":
      return "bg-red-50 border-red-300";
    case "error":
      return "bg-orange-50 border-orange-300";
    case "warning":
      return "bg-yellow-50 border-yellow-300";
    case "info":
      return "bg-blue-50 border-blue-300";
  }
}

function getSeverityBadgeStyles(
  severity: ProjectHealthIssue["severity"]
): string {
  switch (severity) {
    case "fatal":
      return "bg-red-200 text-red-800";
    case "error":
      return "bg-orange-200 text-orange-800";
    case "warning":
      return "bg-yellow-200 text-yellow-800";
    case "info":
      return "bg-blue-200 text-blue-800";
  }
}
