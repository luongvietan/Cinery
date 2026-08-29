import { useEffect, useState } from "react";
import type { CanonEntity } from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import { createCanonEntity, listCanonEntities } from "./api";
import { CharacterEditor } from "./CharacterEditor";
import { CharacterLookPanel } from "./CharacterLookPanel";

export function CharacterList({ projectRootPath, initialSelectedId }: { projectRootPath: string; initialSelectedId?: string | null }) {
  const [characters, setCharacters] = useState<CanonEntity[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(initialSelectedId ?? null);
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  async function load() { try { setCharacters((await listCanonEntities(projectRootPath, "character")) ?? []); setError(null); } catch (caught) { setError(describeError(caught)); } }
  useEffect(() => { void load(); }, [projectRootPath]);
  useEffect(() => { if (initialSelectedId) setSelectedId(initialSelectedId); }, [initialSelectedId]);
  async function create(event: React.FormEvent) { event.preventDefault(); try { const created = await createCanonEntity({ projectRootPath, type: "character", name }); setName(""); setSelectedId(created.id); await load(); } catch (caught) { setError(describeError(caught)); } }
  const selected = characters.find((character) => character.id === selectedId) ?? null;
  return <section aria-label="Characters" className="canon-content"><header className="canon-panel-header"><div><h2>Characters</h2><p>Write who they are, then generate the face, outfit, and sheet every scene reuses.</p></div></header><form onSubmit={create} className="canon-create-form"><label>Name<input value={name} onChange={(event) => setName(event.target.value)} required /></label><button type="submit">Add Character</button></form>{error ? <p role="alert">{error}</p> : null}<div className="canon-entity-layout"><aside><ul className="canon-entity-list">{characters.map((character) => <li key={character.id}><button type="button" className={selectedId === character.id ? "canon-entity-button canon-entity-button--selected" : "canon-entity-button"} onClick={() => setSelectedId(character.id)}>{character.name}</button></li>)}</ul></aside>{selected ? <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-16)" }}><CharacterEditor projectRootPath={projectRootPath} entityId={selected.id} />{selected ? <CharacterLookPanel projectRootPath={projectRootPath} character={{ id: selected.id, name: selected.name }} /> : null}</div> : <div className="empty-state" role="status"><p>No character selected</p><p>Add a character above, then write their story sections and generate their look references below.</p></div>}</div></section>;
}
