import { describe, expect, it } from "vitest";
import {
  cinemaCompilationSchema,
  computeTimeBudget,
  validateTotalDuration,
} from "./cinema.js";

describe("cinema compilation schema", () => {
  it("rejects empty shots", () => {
    expect(() =>
      cinemaCompilationSchema.parse({
        id: "c1",
        projectId: "p",
        sceneId: "s",
        totalDurationSeconds: 8,
        shots: [],
        behavioralLocks: {},
        worldContinuity: {},
        audioInstructions: null,
        providerPrompt: "x",
        createdAt: new Date().toISOString(),
      }),
    ).toThrow();
  });

  it("accepts valid 8s two-shot", () => {
    const v = {
      id: "c1",
      projectId: "p",
      sceneId: "s",
      totalDurationSeconds: 8,
      shots: [
        {
          order: 0,
          durationSeconds: 4,
          intent: "Establish",
          camera: "wide",
          action: "stand",
          continuity: "keep look",
        },
        {
          order: 1,
          durationSeconds: 4,
          intent: "Close",
          camera: "medium",
          action: "look",
          continuity: "keep look",
        },
      ],
      behavioralLocks: { speech: "calm", movement: "precise", stillness: "restrained" },
      worldContinuity: { plateId: "wp-v1", notes: "Station" },
      providerPrompt: "CINEMA PROMPT",
      createdAt: "2026-08-28T00:00:00.000Z",
    };
    expect(cinemaCompilationSchema.parse(v).totalDurationSeconds).toBe(8);
  });

  it("rejects shots whose durations do not sum to the total", () => {
    expect(() =>
      cinemaCompilationSchema.parse({
        id: "c1",
        projectId: "p",
        sceneId: "s",
        totalDurationSeconds: 8,
        shots: [
          { order: 0, durationSeconds: 4, intent: "Establish" },
          { order: 1, durationSeconds: 3, intent: "Close" },
        ],
        behavioralLocks: {},
        worldContinuity: {},
        providerPrompt: "CINEMA PROMPT",
        createdAt: "2026-08-28T00:00:00.000Z",
      }),
    ).toThrow();
  });
});

describe("time budget helpers", () => {
  it("rejects out-of-range totals", () => {
    expect(() => validateTotalDuration(0)).toThrow();
    expect(() => validateTotalDuration(121)).toThrow();
    expect(validateTotalDuration(8)).toBe(8);
  });

  it("auto-budgets 8s as two 4s shots", () => {
    expect(computeTimeBudget(8)).toEqual([4, 4]);
  });

  it("distributes remainder to earlier shots and sums exactly", () => {
    const budget = computeTimeBudget(10, 3);
    expect(budget).toEqual([3.34, 3.33, 3.33]);
    expect(budget.reduce((a, b) => a + b, 0)).toBeCloseTo(10, 6);
  });
});
