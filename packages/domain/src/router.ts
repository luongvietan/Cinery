export interface RoutedOperation {
  skillId: string;
  skillVersion: string;
  operationId: string;
  operationName: string;
  score: number;
  prerequisitePassed: boolean;
  prerequisiteBlockers: string[];
}

export interface RouteProductionIntentResult {
  matched: boolean;
  suggested: RoutedOperation | null;
  candidates: RoutedOperation[];
}
