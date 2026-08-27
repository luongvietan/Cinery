import { useState } from "react";
import { describeError } from "../../lib/errors";
import { exportStoryBible } from "./api";
export function ExportStoryBibleButton({ projectRootPath }: { projectRootPath: string }) { const [message, setMessage] = useState<string | null>(null); const [error, setError] = useState<string | null>(null); async function exportBible() { try { const result = await exportStoryBible(projectRootPath); setMessage(`Exported ${result.relativePath} (${result.byteSize} bytes)`); setError(null); } catch (caught) { setError(describeError(caught)); } } return <div className="canon-export-action"><button type="button" onClick={() => void exportBible()}>Export Story Bible</button>{message ? <span>{message}</span> : null}{error ? <p role="alert">{error}</p> : null}</div>; }
