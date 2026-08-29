import { useEffect, useState } from "react";
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

export function CanonWorkspace({
  projectRootPath,
  initialTab,
  initialCharacterId,
}: {
  projectRootPath: string;
  initialTab?: CanonTab;
  initialCharacterId?: string | null;
}) {
  const [tab, setTab] = useState<CanonTab>("Story");
  useEffect(() => { if (initialTab) setTab(initialTab); }, [initialTab]);
  return (
    <section aria-label="Story workspace" className="canon-workspace">
      <div className="canon-workspace-toolbar">
        <div>
          <h2>Story</h2>
          <p>Who is in your film and what happens. Locked facts here keep every scene consistent.</p>
        </div>
        <ExportStoryBibleButton projectRootPath={projectRootPath} />
      </div>
      <nav className="canon-subnav" aria-label="Story sections">
        {tabs.map((item) => (
          <button type="button" key={item} className={item === tab ? "nav-button nav-button--active" : "nav-button"} onClick={() => setTab(item)}>{item}</button>
        ))}
      </nav>
      {tab === "Story" ? <StoryCanonEditor projectRootPath={projectRootPath} /> : null}
      {tab === "Characters" ? <CharacterList projectRootPath={projectRootPath} initialSelectedId={initialCharacterId} /> : null}
      {tab === "Locations" ? <LocationList projectRootPath={projectRootPath} /> : null}
      {tab === "Factions" ? <FactionList projectRootPath={projectRootPath} /> : null}
      {tab === "World Rules" ? <WorldRulesEditor projectRootPath={projectRootPath} /> : null}
      {tab === "Production Rules" ? <ProductionRulesEditor projectRootPath={projectRootPath} /> : null}
      {tab === "TBDs" ? <TbdPanel projectRootPath={projectRootPath} /> : null}
    </section>
  );
}
