import { useEffect, useState } from "react";
import type { CanonEntity } from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import { createCanonEntity, listCanonEntities } from "./api";
import { CharacterEditor } from "./CharacterEditor";

export function CharacterList({ projectRootPath }: { projectRootPath: string }) {
  const [characters, setCharacters] = useState<CanonEntity[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  async function load() { try { setCharacters((await listCanonEntities(projectRootPath, "character")) ?? []); setError(null); } catch (caught) { setError(describeError(caught)); } }
  useEffect(() => { void load(); }, [projectRootPath]);
  async function create(event: React.FormEvent) { event.preventDefault(); try { const created = await createCanonEntity({ projectRootPath, type: "character", name }); setName(""); setSelectedId(created.id); await load(); } catch (caught) { setError(describeError(caught)); } }
  return <section aria-label="Characters" className="canon-content"><header className="canon-panel-header"><div><h2>Characters</h2><p>Structured character truth and permanent visual locks.</p></div></header><form onSubmit={create} className="canon-create-form"><label>Name<input value={name} onChange={(event) => setName(event.target.value)} required /></label><button type="submit">New Character</button></form>{error ? <p role="alert">{error}</p> : null}<div className="canon-entity-layout"><aside><ul className="canon-entity-list">{characters.map((character) => <li key={character.id}><button type="button" className={selectedId === character.id ? "canon-entity-button canon-entity-button--selected" : "canon-entity-button"} onClick={() => setSelectedId(character.id)}>{character.name}</button></li>)}</ul></aside>{selectedId ? <CharacterEditor projectRootPath={projectRootPath} entityId={selectedId} /> : <p>Select a character.</p>}</div></section>;
}
