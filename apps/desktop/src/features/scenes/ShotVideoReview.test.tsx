import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi, beforeEach } from "vitest";
import { ShotVideoReview } from "./ShotVideoReview";
import {
  listShotVideoCandidates,
  promoteShotVideoCandidate,
  rejectShotVideoCandidate,
  restoreShotVideoCandidate,
  type ShotVideoCandidate,
} from "./api";
import * as qaApi from "../qa/api";

vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `asset://localhost/${path}`,
}));
vi.mock("./api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./api")>();
  return {
    ...actual,
    listShotVideoCandidates: vi.fn(),
    promoteShotVideoCandidate: vi.fn(),
    rejectShotVideoCandidate: vi.fn(),
    restoreShotVideoCandidate: vi.fn(),
  };
});

vi.mock("../qa/api");
vi.mock("../workflows/api", () => ({
  listWorkflowRuns: vi.fn().mockResolvedValue([]),
  getWorkflowRun: vi.fn(),
  listSkillOperations: vi.fn(),
}));
vi.mock("../generation/api", () => ({
  listGenerationResults: vi.fn().mockResolvedValue([]),
}));
function candidate(overrides: Partial<ShotVideoCandidate> = {}): ShotVideoCandidate {
  return {
    assetVersionId: "video-v1",
    versionNumber: 1,
    shotId: "shot-1",
    sceneId: "scene-1",
    createdAt: "2026-09-03T00:00:00Z",
    filePath: "assets/video-v1.mp4",
    mimeType: "video/mp4",
    byteSize: 24000,
    reviewState: "active",
    isCanonical: false,
    qaOverallStatus: null,
    qaRunCount: 0,
    providerId: "i2v",
    modelId: "motion-v1",
    workflowRunId: "run-1",
    sourceAssetVersionId: "kf-v1",
    sourceKeyframeIsCurrent: true,
    ...overrides,
  };
}

describe("ShotVideoReview (P10.4)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(listShotVideoCandidates).mockReset();
    vi.mocked(listShotVideoCandidates).mockResolvedValue([]);
    vi.mocked(qaApi.listQaRuns).mockReset();
    vi.mocked(qaApi.listQaRuns).mockResolvedValue([]);
  });

  it("renders an empty state when the Shot has no candidates", async () => {
    render(<ShotVideoReview projectRootPath="C:/project" shotId="shot-1" onChanged={vi.fn()} />);
    expect(await screen.findByRole("status")).toHaveTextContent(/No generated video candidates/);
  });

  it("renders candidates newest first with QA, timestamps, and video previews", async () => {
    vi.mocked(listShotVideoCandidates).mockResolvedValue([
      candidate({ assetVersionId: "video-v2", versionNumber: 2, qaRunCount: 1, qaOverallStatus: "pass" }),
      candidate({ assetVersionId: "video-v1", versionNumber: 1, qaRunCount: 0 }),
    ]);
    render(<ShotVideoReview projectRootPath="C:/project" shotId="shot-1" onChanged={vi.fn()} />);

    const v02 = await screen.findByRole("button", { name: "V02" });
    const v01 = screen.getByRole("button", { name: "V01" });
    // Document order: V02 before V01 (newest first).
    expect(v02.compareDocumentPosition(v01) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.getByText("QA: passed")).toBeInTheDocument();
    expect(screen.getByText("QA: not run")).toBeInTheDocument();
    const videos = document.querySelectorAll("video");
    expect(videos.length).toBeGreaterThanOrEqual(2);
    for (const video of videos) {
      expect(video).toHaveAttribute("controls");
      expect(video).not.toHaveAttribute("autoplay");
    }
  });

  it("shows the canonical badge from the backend state, not from ordering", async () => {
    vi.mocked(listShotVideoCandidates).mockResolvedValue([
      candidate({ assetVersionId: "video-v2", versionNumber: 2 }),
      candidate({ assetVersionId: "video-v1", versionNumber: 1, isCanonical: true }),
    ]);
    render(<ShotVideoReview projectRootPath="C:/project" shotId="shot-1" onChanged={vi.fn()} />);
    expect(await screen.findByTestId("canonical-badge-video-v1")).toBeInTheDocument();
    expect(screen.queryByTestId("canonical-badge-video-v2")).not.toBeInTheDocument();
  });

  it("exposes provenance and detail on selection", async () => {
    vi.mocked(listShotVideoCandidates).mockResolvedValue([
      candidate({ qaOverallStatus: "fail", qaRunCount: 1 }),
    ]);
    const user = userEvent.setup();
    render(<ShotVideoReview projectRootPath="C:/project" shotId="shot-1" onChanged={vi.fn()} />);
    await user.click(await screen.findByRole("button", { name: "V01" }));
    expect(await screen.findByText(/i2v · motion-v1/)).toBeInTheDocument();
    expect(screen.getByText("QA: failed")).toBeInTheDocument();
  });

  it("rejects an active noncanonical candidate and restores a rejected one", async () => {
    vi.mocked(listShotVideoCandidates).mockResolvedValue([candidate()]);
    vi.mocked(rejectShotVideoCandidate).mockResolvedValue("rejected");
    vi.mocked(restoreShotVideoCandidate).mockResolvedValue("active");
    vi.mocked(listShotVideoCandidates)
      .mockResolvedValueOnce([candidate()])
      .mockResolvedValueOnce([candidate({ reviewState: "rejected" })])
      .mockResolvedValueOnce([candidate()]);
    const user = userEvent.setup();
    const onChanged = vi.fn();
    render(<ShotVideoReview projectRootPath="C:/project" shotId="shot-1" onChanged={onChanged} />);

    await user.click(await screen.findByRole("button", { name: "V01" }));
    await user.click(screen.getByRole("button", { name: "Reject" }));
    await waitFor(() => expect(rejectShotVideoCandidate).toHaveBeenCalledWith(
      "C:/project", "shot-1", "video-v1", null,
    ));
    await waitFor(() => expect(onChanged).toHaveBeenCalled());
  });

  it("promotes with confirmation and passes the expected canonical for concurrency safety", async () => {
    vi.mocked(listShotVideoCandidates).mockResolvedValue([
      candidate({ assetVersionId: "video-v1", versionNumber: 1, isCanonical: true }),
      candidate({ assetVersionId: "video-v2", versionNumber: 2 }),
    ]);
    vi.mocked(promoteShotVideoCandidate).mockResolvedValue({
      shotId: "shot-1",
      artifactId: "artifact-2",
      assetVersionId: "video-v2",
      previousAssetVersionId: "video-v1",
    });
    const user = userEvent.setup();
    const onChanged = vi.fn();
    render(<ShotVideoReview projectRootPath="C:/project" shotId="shot-1" onChanged={onChanged} />);

    await user.click(await screen.findByRole("button", { name: "V02" }));
    await user.click(screen.getByRole("button", { name: "Promote" }));

    // Replacement warning names both versions.
    const dialog = screen.getByRole("dialog");
    expect(dialog).toHaveTextContent(/Make V02 the canonical video/);
    expect(dialog).toHaveTextContent(/replaces V01 as the canonical selection/);
    expect(dialog).toHaveTextContent(/V01 will remain available/);

    await user.click(within(dialog).getByRole("button", { name: "Promote" }));
    await waitFor(() => expect(promoteShotVideoCandidate).toHaveBeenCalledWith(
      "C:/project", "shot-1", "video-v2", "video-v1", null,
    ));
    await waitFor(() => expect(onChanged).toHaveBeenCalled());
  });

  it("does not offer Reject for the canonical candidate", async () => {
    vi.mocked(listShotVideoCandidates).mockResolvedValue([
      candidate({ isCanonical: true }),
    ]);
    const user = userEvent.setup();
    render(<ShotVideoReview projectRootPath="C:/project" shotId="shot-1" onChanged={vi.fn()} />);
    await user.click(await screen.findByRole("button", { name: "V01" }));
    expect(screen.queryByRole("button", { name: "Reject" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Canonical ✓" })).toBeDisabled();
  });

  it("offers Restore (not Promote) for a rejected candidate", async () => {
    vi.mocked(listShotVideoCandidates).mockResolvedValue([
      candidate({ reviewState: "rejected" }),
    ]);
    vi.mocked(restoreShotVideoCandidate).mockResolvedValue("active");
    const user = userEvent.setup();
    render(<ShotVideoReview projectRootPath="C:/project" shotId="shot-1" onChanged={vi.fn()} />);
    await user.click(await screen.findByRole("button", { name: "V01" }));
    expect(await screen.findByText("Rejected")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Restore" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Promote" })).not.toBeInTheDocument();
  });

  it("requires an explicit override with a reason for a QA-failed candidate", async () => {
    vi.mocked(listShotVideoCandidates).mockResolvedValue([
      candidate({ qaOverallStatus: "fail", qaRunCount: 1 }),
    ]);
    vi.mocked(promoteShotVideoCandidate).mockResolvedValue({
      shotId: "shot-1",
      artifactId: "artifact-1",
      assetVersionId: "video-v1",
      previousAssetVersionId: null,
    });
    const user = userEvent.setup();
    render(<ShotVideoReview projectRootPath="C:/project" shotId="shot-1" onChanged={vi.fn()} />);

    await user.click(await screen.findByRole("button", { name: "V01" }));
    await user.click(screen.getByRole("button", { name: "Promote" }));

    const dialog = screen.getByRole("dialog");
    expect(dialog).toHaveTextContent(/Video QA reported a failure/);
    const promoteAnyway = within(dialog).getByRole("button", { name: "Promote anyway" });
    // Blocked until the acknowledgement + reason are provided.
    expect(promoteAnyway).toBeDisabled();

    await user.click(within(dialog).getByLabelText(/I understand the reported issue/));
    expect(promoteAnyway).toBeDisabled();
    await user.type(
      within(dialog).getByLabelText(/Why are you promoting this candidate anyway/),
      "Director approved this take.",
    );
    expect(promoteAnyway).toBeEnabled();
    await user.click(promoteAnyway);

    await waitFor(() => expect(promoteShotVideoCandidate).toHaveBeenCalledWith(
      "C:/project", "shot-1", "video-v1", null, "Director approved this take.",
    ));
  });

  it("requires an override for a stale keyframe candidate", async () => {
    vi.mocked(listShotVideoCandidates).mockResolvedValue([
      candidate({ sourceKeyframeIsCurrent: false }),
    ]);
    const user = userEvent.setup();
    render(<ShotVideoReview projectRootPath="C:/project" shotId="shot-1" onChanged={vi.fn()} />);

    await user.click(await screen.findByRole("button", { name: "V01" }));
    await user.click(screen.getByRole("button", { name: "Promote" }));
    const dialog = screen.getByRole("dialog");
    expect(dialog).toHaveTextContent(/generated from an earlier keyframe/);
    expect(within(dialog).getByRole("button", { name: "Promote anyway" })).toBeDisabled();
  });

  it("surfaces a canonical conflict by reloading state instead of retrying", async () => {
    vi.mocked(listShotVideoCandidates).mockResolvedValue([
      candidate({ assetVersionId: "video-v2", versionNumber: 2 }),
    ]);
    vi.mocked(promoteShotVideoCandidate).mockRejectedValue({
      code: "PROMOTION_CONFLICT",
      message: "the Shot video changed before promotion completed",
    });
    const user = userEvent.setup();
    render(<ShotVideoReview projectRootPath="C:/project" shotId="shot-1" onChanged={vi.fn()} />);

    await user.click(await screen.findByRole("button", { name: "V02" }));
    await user.click(screen.getByRole("button", { name: "Promote" }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Promote" }));

    await waitFor(() => {
      const alerts = screen.getAllByRole("alert");
      expect(
        alerts.some((alert) => alert.textContent?.includes("canonical video changed")),
      ).toBe(true);
    });
    // State was reloaded (second read) and the dialog is closed: no blind retry.
    await waitFor(() => expect(listShotVideoCandidates).toHaveBeenCalledTimes(2));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();  });

  it("compares two candidates side by side without mutating them", async () => {
    vi.mocked(listShotVideoCandidates).mockResolvedValue([
      candidate({ assetVersionId: "video-v1", versionNumber: 1, isCanonical: true, qaOverallStatus: "pass", qaRunCount: 1 }),
      candidate({ assetVersionId: "video-v2", versionNumber: 2, qaOverallStatus: "fail", qaRunCount: 1 }),
    ]);
    const user = userEvent.setup();
    render(<ShotVideoReview projectRootPath="C:/project" shotId="shot-1" onChanged={vi.fn()} />);

    await user.click(await screen.findByRole("button", { name: "V02" }));
    await user.click(screen.getByRole("button", { name: "Compare" }));

    const compare = await screen.findByTestId("compare-view");
    expect(compare).toHaveTextContent("Compare A: V01");
    expect(compare).toHaveTextContent("Compare B: V02");
    expect(compare).toHaveTextContent("Canonical ✓");
    // jsdom: video has no implicit ARIA role — query by element.
    expect(compare.querySelectorAll("video").length).toBe(2);
    // Comparison alone triggers no mutations.
    expect(promoteShotVideoCandidate).not.toHaveBeenCalled();
    expect(rejectShotVideoCandidate).not.toHaveBeenCalled();
  });
});
