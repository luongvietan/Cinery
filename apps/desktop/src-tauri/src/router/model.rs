use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutedOperation {
    pub skill_id: String,
    pub skill_version: String,
    pub operation_id: String,
    pub operation_name: String,
    pub score: i32,
    pub prerequisite_passed: bool,
    pub prerequisite_blockers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteProductionIntentRequest {
    pub project_root_path: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteProductionIntentResult {
    pub matched: bool,
    pub suggested: Option<RoutedOperation>,
    pub candidates: Vec<RoutedOperation>,
}
