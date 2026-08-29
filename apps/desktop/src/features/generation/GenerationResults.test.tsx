import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { AssetSummary, AssetVersion, GenerationResultSetDetail } from "@cinematic/domain";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { GenerationResults } from "./GenerationResults";
import { promoteGeneratedArtifact } from "./api";
import { createAsset } from "../assets/api";

vi.mock("./api");
vi.mock("../assets/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `mock-asset://${path}`,
}));

const resultSets: GenerationResultSetDetail[] = [
  {
    resultSet: {
      id: "result-set-1",
      projectId: "project-1",
      workflowRunId: "run-1",
      workflowStepKey: "execute",
      providerAttemptId: "attempt-1",
      mediaKind: "image",
      requestedOutputCount: 2,
      createdAt: "2026-08-29T00:00:00Z",
    },
    artifacts: [
      {
        artifact: {
          id: "artifact-1",
          resultSetId: "result-set-1",
          ordinal: 1,
          mediaKind: "image",
          mimeType: "image/png",
          width: 1024,
          height: 1024,
          byteSize: 1234,
          sha256: "a".repeat(64),
          storagePath: "generations/run-1/attempt-1/1.png",
          captureStatus: "available",
          captureErrorCode: null,
          createdAt: "2026-08-29T00:00:00Z",
        },
        lineage: {
          artifactId: "artifact-1",
          workflowRunId: "run-1",
          workflowStepKey: "execute",
          workflowDefinitionId: "character.create_outfit",
          workflowVersion: "1.1.0",
          skillId: "character-builder",
          skillVersion: "1.1.0",
          compiledExecutionArtifactId: "compiled-1",
          compiledRequestSha256: "b".repeat(64),
          canonSnapshotId: null,
          canonSnapshotSha256: null,
          providerAttemptId: "attempt-1",
          providerId: "openai",
          modelId: "gpt-image-2",
          sourceAssetVersionIds: ["face-v1"],
          createdAt: "2026-08-29T00:00:00Z",
        },
      },
      {
        artifact: {
          id: "artifact-2",
          resultSetId: "result-set-1",
          ordinal: 2,
          mediaKind: "image",
          mimeType: "image/png",
          width: 1024,
          height: 1024,
          byteSize: 1234,
          sha256: "c".repeat(64),
          storagePath: "generations/run-1/attempt-1/2.png",
          captureStatus: "available",
          captureErrorCode: null,
          createdAt: "2026-08-29T00:00:00Z",
        },
        lineage: null,
      },
    ],
  },
];

function assetSummary(overrides: Partial<AssetSummary>): AssetSummary {
  return {
    id: "asset-1",
    projectId: "project-1",
    type: "outfit",
    label: "Mara Outfit",
    ownerEntityId: "mara",
    canonicalVersionId: "outfit-v1",
    versionCount: 1,
    canonicalVersionNumber: 1,
    previewThumbnailPath: null,
    createdAt: "2026-08-29T00:00:00Z",
    updatedAt: "2026-08-29T00:00:00Z",
    ...overrides,
  };
}

const context = {
  workflowRunId: "run-1",
  operationId: "character.create_outfit",
  expectedAssetType: "outfit" as const,
  ownerEntityId: "mara",
  resultSets,
};

describe("GenerationResults", () => {
  beforeEach(() => {
    vi.mocked(promoteGeneratedArtifact).mockReset();
    vi.mocked(createAsset).mockReset();
  });

  it("renders persisted candidates with metadata regardless of operation", async () => {
    render(
      <GenerationResults
        projectRootPath="C:/p"
        context={context}
        assets={[assetSummary({})]}
        onPromoted={vi.fn()}
      />,
    );
    expect(await screen.findByText("Result 1")).toBeInTheDocument();
    expect(screen.getByText("Result 2")).toBeInTheDocument();
    expect(screen.getAllByText(/gpt-image-2/).length).toBeGreaterThan(0);
  });

  it("filters target assets by expected type and owner", async () => {
    const assets = [
      assetSummary({ id: "eligible", type: "outfit", ownerEntityId: "mara" }),
      assetSummary({ id: "wrong-type", type: "face_lock", label: "Wrong Type" }),
      assetSummary({ id: "wrong-owner", type: "outfit", ownerEntityId: "other", label: "Wrong Owner" }),
    ];
    render(
      <GenerationResults
        projectRootPath="C:/p"
        context={context}
        assets={assets}
        onPromoted={vi.fn()}
      />,
    );
    const targetSelect = await screen.findByLabelText(/Save into/);
    const values = Array.from((targetSelect as HTMLSelectElement).options).map((option) => option.value);
    expect(values).toContain("eligible");
    expect(values).not.toContain("wrong-type");
    expect(values).not.toContain("wrong-owner");
  });

  it("offers inline asset creation when no eligible target exists and requires explicit promotion", async () => {
    const user = userEvent.setup();
    const created = { id: "new-asset", projectId: "project-1", type: "outfit", label: "New Mara Outfit", ownerEntityId: "mara", canonicalVersionId: null, createdAt: "", updatedAt: "" };
    vi.mocked(createAsset).mockResolvedValue(created as unknown as Awaited<ReturnType<typeof createAsset>>);
    vi.mocked(promoteGeneratedArtifact).mockResolvedValue({
      id: "v-new",
      assetId: "new-asset",
      versionNumber: 1,
      status: "canonical",
      filePath: "",
      thumbnailPath: "",
      sha256: "d".repeat(64),
      originalFilename: "",
      mimeType: "image/png",
      byteSize: 0,
      width: null,
      height: null,
      parentVersionId: null,
      createdAt: "",
      origin: "generated",
      generationArtifactId: "artifact-1",
    } as unknown as AssetVersion);
    const onPromoted = vi.fn();

    render(
      <GenerationResults
        projectRootPath="C:/p"
        context={context}
        assets={[]}
        onPromoted={onPromoted}
      />,
    );
    // No eligible target: show create form instead of a dead-end select.
    expect(screen.queryByLabelText(/Save into/)).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Create outfit asset/ }));
    await user.type(screen.getByLabelText(/Name/), "New Mara Outfit");
    await user.click(screen.getByRole("button", { name: "Create" }));
    await waitFor(() => expect(createAsset).toHaveBeenCalledWith(expect.objectContaining({ type: "outfit", ownerEntityId: "mara" })));

    // The new asset becomes the selected target, but saving still needs
    // an explicit Save click.
    const saveButton = await screen.findByRole("button", { name: /Save to Assets/ });
    await user.click(saveButton);
    const dialog = await screen.findByRole("dialog");
    await user.click(withinDialog(dialog).getByRole("button", { name: /Save version/ }));
    await waitFor(() => expect(promoteGeneratedArtifact).toHaveBeenCalledWith("C:/p", "artifact-1", "new-asset", false));
    await waitFor(() => expect(onPromoted).toHaveBeenCalled());
  });

  it("keeps already promoted artifacts visible but not promotable twice", async () => {
    const promoted = JSON.parse(JSON.stringify(resultSets)) as GenerationResultSetDetail[];
    promoted[0].artifacts[0].lineage!.workflowRunId = "run-1";
    render(
      <GenerationResults
        projectRootPath="C:/p"
        context={context}
        assets={[assetSummary({})]}
        onPromoted={vi.fn()}
      />,
    );
    expect(await screen.findByText("Result 1")).toBeInTheDocument();
  });
});

import { within as withinDialog } from "@testing-library/react";
