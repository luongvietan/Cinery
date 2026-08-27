import { useState } from "react";
import { StoryCanonEditor } from "./StoryCanonEditor";
import { CharacterList } from "./CharacterList";
import { LocationList } from "./LocationList";
import { FactionList } from "./FactionList";
import { WorldRulesEditor } from "./WorldRulesEditor";
import { ProductionRulesEditor } from "./EntityCategoryEditor";
import { TbdPanel } from "./TbdPanel";
import { ExportStoryBibleButton } from "./ExportStoryBibleButton";

type CanonTab = "Story" | "Characters" | "Locations" | "Factions" | "World Rules" | "Production Rules" | "TBDs";
const tabs: CanonTab[] = ["Story", "Characters", "Locations", "Factions", "World Rules", "Production Rules", "TBDs"];

export function CanonWorkspace({ projectRootPath }: { projectRootPath: string }) {
  const [tab, setTab] = useState<CanonTab>("Story");
  return <section aria-label="Canon workspace" className="canon-workspace"><div className="canon-workspace-toolbar"><nav className="canon-subnav">{tabs.map((item) => <button type="button" key={item} className={item === tab ? "nav-button nav-button--active" : "nav-button"} onClick={() => setTab(item)}>{item}</button>)}</nav><ExportStoryBibleButton projectRootPath={projectRootPath} /></div>{tab === "Story" ? <StoryCanonEditor projectRootPath={projectRootPath} /> : null}{tab === "Characters" ? <CharacterList projectRootPath={projectRootPath} /> : null}{tab === "Locations" ? <LocationList projectRootPath={projectRootPath} /> : null}{tab === "Factions" ? <FactionList projectRootPath={projectRootPath} /> : null}{tab === "World Rules" ? <WorldRulesEditor projectRootPath={projectRootPath} /> : null}{tab === "Production Rules" ? <ProductionRulesEditor projectRootPath={projectRootPath} /> : null}{tab === "TBDs" ? <TbdPanel projectRootPath={projectRootPath} /> : null}</section>;
}
