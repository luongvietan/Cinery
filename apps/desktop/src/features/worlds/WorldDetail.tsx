import { useEffect, useState } from "react";
import type { CanonEntityDetail, CanonTbd } from "@cinematic/domain";
import { describeError } from "../../lib/errors";
import { getCanonEntity, listCanonTbds } from "../canon/api";
import { BackButton } from "../../components/BackButton";
import { getWorldDetailed } from "./api";
import type { WorldDetail as WorldDetailType } from "./types";
import { WorldPlatePanel } from "./WorldPlatePanel";

interface WorldDetailProps {
  projectRootPath: string;
  worldId: string;
  onBack?: () => void;
}

export function WorldDetail({
  projectRootPath,
  worldId,
  onBack,
}: WorldDetailProps) {
  const [detail, setDetail] = useState<WorldDetailType | null>(null);
  const [canonDetail, setCanonDetail] = useState<CanonEntityDetail | null>(
    null,
  );
  const [tbds, setTbds] = useState<CanonTbd[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setDetail(null);
    setCanonDetail(null);
    Promise.resolve()
      .then(() => getWorldDetailed(projectRootPath, worldId))
      .then((worldDetail) => {
        if (cancelled) return null;
        setDetail(worldDetail);
        return Promise.all([
          getCanonEntity(projectRootPath, worldDetail.location.id),
          listCanonTbds(projectRootPath),
        ]);
      })
      .then((results) => {
        if (cancelled || !results) return;
        const [canon, allTbds] = results;
        setCanonDetail(canon);
        // Filter protected TBDs scoped to this location or project
        const relevant = allTbds.filter(
          (tbd) =>
            tbd.protected &&
            tbd.status === "open" &&
            (tbd.canonEntityId === canon.entity.id ||
              tbd.canonEntityId === null),
        );
        setTbds(relevant);
      })
      .catch((caught: unknown) => {
        if (!cancelled) setError(describeError(caught));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [projectRootPath, worldId]);

  if (loading) {
    return <p role="status">Loading world…</p>;
  }

  if (error) {
    return <p role="alert">{error}</p>;
  }

  if (!detail) {
    return <p role="alert">World not found.</p>;
  }

  const sectionsByKey = new Map(
    (canonDetail?.sections ?? []).map((section) => [section.key, section]),
  );

  const sectionRow = (key: string, title: string) => {
    const section = sectionsByKey.get(key) as unknown as
      | { status: string; revision: number }
      | undefined;
    const status = section?.status ?? "draft";
    return (
      <div
        key={key}
        style={{
          display: "flex",
          justifyContent: "space-between",
          padding: "var(--space-8) 0",
          borderBottom: "1px solid var(--c-hairline)",
        }}
      >
        <span style={{ fontWeight: 500 }}>{title}</span>
        <span
          className={
            status === "locked"
              ? "canon-status canon-status--locked"
              : "canon-status canon-status--draft"
          }
        >
          {status.toUpperCase()}
        </span>
      </div>
    );
  };

  return (
    <section
      aria-label={`World ${detail.location.name}`}
      className="canon-content"
    >
      {onBack ? <BackButton label="← Worlds" onClick={onBack} /> : null}
      <header className="canon-panel-header">
        <div>
          <h2 style={{ textTransform: "uppercase" }}>{detail.location.name}</h2>
          <p>World id {detail.world.id} · Location {detail.location.id}</p>
        </div>
      </header>

      {/* Canon Location lock state */}
      <section aria-label="Canon Location" style={{ marginBottom: "var(--space-16)" }}>
        <h3>CANON LOCATION</h3>
        <div style={{ marginTop: "var(--space-8)" }}>
          {sectionRow("description", "Description")}
          {sectionRow("geography", "Geography")}
          {sectionRow("visual_tags", "Visual Tags")}
          {sectionRow("rules", "Rules")}
        </div>
        <p style={{ marginTop: "var(--space-8)", fontSize: "var(--fs-sm)", color: "var(--c-muted)" }}>
          Description and Geography must be LOCKED before generating a World Plate.
          Draft Canon is not generation authority.
        </p>
      </section>

      {/* World Plate */}
      <WorldPlatePanel projectRootPath={projectRootPath} detail={detail} />

      {/* Protected Unknowns */}
      <section
        aria-label="Protected Unknowns"
        style={{
          marginTop: "var(--space-16)",
          padding: "var(--space-16)",
          background: "var(--c-panel)",
          border: "1px solid var(--c-hairline)",
          borderRadius: "var(--radius-lg)",
        }}
      >
        <h3>Protected Unknowns</h3>
        {tbds.length === 0 ? (
          <p>No protected unknowns for this World.</p>
        ) : (
          <ul style={{ listStyle: "none", padding: 0, margin: "var(--space-8) 0 0" }}>
            {tbds.map((tbd) => (
              <li
                key={tbd.id}
                style={{
                  padding: "var(--space-8) 0",
                  borderBottom: "1px solid var(--c-hairline)",
                }}
              >
                <div style={{ display: "flex", gap: "var(--space-8)", alignItems: "baseline" }}>
                  <span aria-hidden="true">•</span>
                  <strong>{tbd.topic}</strong>
                  <span className="canon-badge">PROTECTED</span>
                  <span style={{ fontSize: "var(--fs-xs)", textTransform: "uppercase", color: "var(--c-muted)" }}>
                    PRESERVE UNKNOWN
                  </span>
                </div>
                {tbd.note ? (
                  <p style={{ marginLeft: "var(--space-16)", fontSize: "var(--fs-md)" }}>{tbd.note}</p>
                ) : null}
              </li>
            ))}
          </ul>
        )}
        <p style={{ marginTop: "var(--space-8)", fontSize: "var(--fs-sm)", color: "var(--c-muted)" }}>
          TBDs scoped to this Location must be preserve_unknown. Project-scoped unknowns require explicit classification.
        </p>
      </section>
    </section>
  );
}
