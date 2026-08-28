export interface DiagnosticsFile {
  name: string;
  content: string;
}

export interface DiagnosticsBundle {
  fileName: string;
  exportedAt: string;
  files: DiagnosticsFile[];
  outputPath: string;
}
