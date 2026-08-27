import { useEffect, useMemo, useState } from "react";
import type { CanonEntity, CanonSection, CanonSectionStatus, CanonEntityDetail } from "@cinematic/domain";
import { parseCanonSectionValue, type AestheticValue, type ActiveSkillRulesValue, type PremiseValue, type RelationshipsValue, type StructuralEnginesValue, type ThesisValue, type TimelineValue } from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import { getCanonEntity, ensureCanonSingletons, lockCanonSection, unlockCanonSection, upsertCanonSection } from "./api";
import { CanonHistoryDialog } from "./CanonHistoryDialog";
import { CanonSectionCard } from "./CanonSectionCard";

interface StoryCanonEditorProps { projectRootPath: string; }

const sectionTitles: Array<[string, string]> = [
  ["premise", "Premise"], ["thesis", "Thesis"], ["timeline", "Timeline"], ["aesthetic", "Aesthetic"],
  ["relationships", "Relationships"], ["structural_engines", "Structural Engines"], ["active_skill_rules", "Active Skill Rules"],
];

const defaults: Record<string, unknown> = {
  premise: { text: "" }, thesis: { text: "" }, relationships: { text: "" }, active_skill_rules: { text: "" },
  timeline: { entries: [] }, aesthetic: { visualRegister: "", palette: [], materials: [], lighting: "", atmosphere: "", exteriorPresence: "", anomalyRule: "", notes: [] },
  structural_engines: { engines: [] },
};

export function StoryCanonEditor({ projectRootPath }: StoryCanonEditorProps) {
  const [story, setStory] = useState<CanonEntity | null>(null);
  const [detail, setDetail] = useState<CanonEntityDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [history, setHistory] = useState<{ id: string; title: string } | null>(null);

  async function load() {
    try {
      const singletons = await ensureCanonSingletons(projectRootPath);
      setStory(singletons.story);
      setDetail(await getCanonEntity(projectRootPath, singletons.story.id));
      setError(null);
    } catch (caught) { setError(describeError(caught)); }
  }
  useEffect(() => { void load(); }, [projectRootPath]);

  const sections = useMemo(() => new Map((detail?.sections ?? []).map((section) => [section.key, section])), [detail]);
  if (error && !story) return <section aria-label="Story Canon"><p role="alert">{error}</p></section>;
  if (!story || !detail) return <section aria-label="Story Canon"><p>Loading Story Canon…</p></section>;
  const storyId = story.id;

  async function save(sectionKey: string, value: unknown) {
    await upsertCanonSection({ projectRootPath, entityId: storyId, sectionKey, value });
    await load();
  }
  async function lock(section: CanonSection<unknown>) { await lockCanonSection({ projectRootPath, sectionId: section.id }); await load(); }
  async function unlock(section: CanonSection<unknown>) { await unlockCanonSection({ projectRootPath, sectionId: section.id }); await load(); }

  return (
    <section aria-label="Story Canon" className="canon-content">
      <header className="canon-panel-header"><div><h2>Story Canon</h2><p>Structured narrative truth. Lock each section independently.</p></div></header>
      {error ? <p role="alert">{error}</p> : null}
      <div className="canon-section-grid">
        {sectionTitles.map(([key, title]) => {
          const section = sections.get(key) as CanonSection<unknown> | undefined;
          const value = section?.value ?? defaults[key];
          return <StorySection key={`${key}-${section?.revision ?? 0}-${section?.status ?? "draft"}`} title={title} sectionKey={key} section={section ?? null} value={value} onSave={save} onLock={lock} onUnlock={unlock} onHistory={(id) => setHistory({ id, title })} />;
        })}
      </div>
      <CanonHistoryDialog projectRootPath={projectRootPath} sectionId={history?.id ?? null} sectionTitle={history?.title ?? "Section"} onClose={() => setHistory(null)} />
    </section>
  );
}

interface StorySectionProps { title: string; sectionKey: string; section: CanonSection<unknown> | null; value: unknown; onSave: (key: string, value: unknown) => Promise<void>; onLock: (section: CanonSection<unknown>) => Promise<void>; onUnlock: (section: CanonSection<unknown>) => Promise<void>; onHistory: (id: string) => void; }

function StorySection({ title, sectionKey, section, value, onSave, onLock, onUnlock, onHistory }: StorySectionProps) {
  const validate = (input: unknown) => parseCanonSectionValue("story", sectionKey, input);
  const textSection = ["premise", "thesis", "relationships", "active_skill_rules"].includes(sectionKey);
  return (
    <CanonSectionCard
      title={title}
      section={section}
      draftValue={value}
      validate={validate}
      renderEditor={(draft, setDraft) => textSection ? <textarea value={(draft as { text: string }).text} onChange={(event) => setDraft({ text: event.target.value })} rows={5} /> : <StructuredEditor sectionKey={sectionKey} value={draft} onChange={setDraft} />}
      renderReadOnly={(current) => textSection ? <p>{(current as { text: string }).text || "[TBD]"}</p> : <StructuredReadOnly sectionKey={sectionKey} value={current} />}
      onSave={(next) => onSave(sectionKey, next)}
      onLock={() => section ? onLock(section) : Promise.resolve()}
      onUnlock={() => section ? onUnlock(section) : Promise.resolve()}
      onHistory={() => section && onHistory(section.id)}
    />
  );
}

function StructuredEditor({ sectionKey, value, onChange }: { sectionKey: string; value: unknown; onChange: (value: unknown) => void }) {
  if (sectionKey === "timeline") { const current = value as TimelineValue; return <ListFields labels={["label", "description"]} rows={current.entries} create={() => ({ id: crypto.randomUUID(), label: "", description: "" })} onChange={(entries) => onChange({ entries })} />; }
  if (sectionKey === "structural_engines") { const current = value as StructuralEnginesValue; return <ListFields labels={["title", "description"]} rows={current.engines} create={() => ({ id: crypto.randomUUID(), title: "", description: "" })} onChange={(engines) => onChange({ engines })} />; }
  const current = value as AestheticValue;
  return <div className="canon-field-grid">{(["visualRegister", "lighting", "atmosphere", "exteriorPresence", "anomalyRule"] as const).map((field) => <label key={field}>{field}<input value={current[field]} onChange={(event) => onChange({ ...current, [field]: event.target.value })} /></label>)}<label>Palette<input value={current.palette.join(", ")} onChange={(event) => onChange({ ...current, palette: event.target.value.split(",").map((item) => item.trim()).filter(Boolean) })} /></label><label>Materials<input value={current.materials.join(", ")} onChange={(event) => onChange({ ...current, materials: event.target.value.split(",").map((item) => item.trim()).filter(Boolean) })} /></label><label>Notes<textarea value={current.notes.join("\n")} onChange={(event) => onChange({ ...current, notes: event.target.value.split("\n") })} /></label></div>;
}

function ListFields<T extends { id: string }>({ labels, rows, create, onChange }: { labels: string[]; rows: T[]; create: () => T; onChange: (rows: T[]) => void }) {
  return <div className="canon-list-editor">{rows.map((row, index) => <div className="canon-list-row" key={row.id}>{labels.map((label) => <label key={label}>{label}<input value={String((row as Record<string, unknown>)[label] ?? "")} onChange={(event) => { const next = [...rows]; next[index] = { ...row, [label]: event.target.value }; onChange(next); }} /></label>)}<button type="button" className="canon-secondary-button" onClick={() => onChange(rows.filter((_, rowIndex) => rowIndex !== index))}>Remove</button></div>)}<button type="button" className="canon-secondary-button" onClick={() => onChange([...rows, create()])}>Add row</button></div>;
}

function StructuredReadOnly({ sectionKey, value }: { sectionKey: string; value: unknown }) {
  if (sectionKey === "timeline") return <ul>{(value as TimelineValue).entries.map((entry) => <li key={entry.id}><strong>{entry.label}</strong> — {entry.description}</li>)}</ul>;
  if (sectionKey === "structural_engines") return <ul>{(value as StructuralEnginesValue).engines.map((engine) => <li key={engine.id}><strong>{engine.title}</strong> — {engine.description}</li>)}</ul>;
  const current = value as AestheticValue;
  return <dl>{Object.entries(current).map(([key, item]) => <div key={key}><dt>{key}</dt><dd>{Array.isArray(item) ? item.join(", ") : String(item || "[TBD]")}</dd></div>)}</dl>;
}
