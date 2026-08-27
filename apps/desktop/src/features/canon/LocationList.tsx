import { EntityCategoryEditor } from "./EntityCategoryEditor";

export function LocationList({ projectRootPath }: { projectRootPath: string }) {
  return (
    <EntityCategoryEditor
      projectRootPath={projectRootPath}
      config={{
        type: "location",
        label: "Locations",
        sections: [
          { key: "description", title: "Description", kind: "text" },
          { key: "visual_tags", title: "Visual Tags", kind: "list" },
          { key: "geography", title: "Geography", kind: "text" },
          { key: "rules", title: "Rules", kind: "rules" },
        ],
      }}
    />
  );
}
