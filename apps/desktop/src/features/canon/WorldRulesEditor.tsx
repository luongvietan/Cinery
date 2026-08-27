import { EntityCategoryEditor } from "./EntityCategoryEditor";
export function WorldRulesEditor({ projectRootPath }: { projectRootPath: string }) { return <EntityCategoryEditor projectRootPath={projectRootPath} config={{ type: "world_rule", label: "World Rules", sections: [{ key: "rule", title: "Rule", kind: "text" }, { key: "notes", title: "Notes", kind: "text" }] }} />; }
