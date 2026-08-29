// Plain-language copy for the golden-path readiness steps. The backend
// reports each step by id; we translate production jargon into what the
// user is actually doing and why it matters.

interface StepCopy {
  title: string;
  detail: string;
  actionLabel: string;
}

const STEP_COPY: Record<string, StepCopy> = {
  story_canon: {
    title: "Set up your story and cast",
    detail: "Add who is in your film and what happens. Everything else builds on this.",
    actionLabel: "Open Story",
  },
  face_lock: {
    title: "Lock each character's face",
    detail: "Generate one approved face reference per character so they look the same in every shot.",
    actionLabel: "Generate faces",
  },
  character_look: {
    title: "Lock each character's outfit",
    detail: "Approve one outfit look per character. Scenes reuse it so nothing drifts.",
    actionLabel: "Generate outfits",
  },
  character_sheet: {
    title: "Create character sheets",
    detail: "A full-body reference sheet gives every scene an exact view of the character.",
    actionLabel: "Generate sheets",
  },
  world_plate: {
    title: "Create your world backdrop",
    detail: "Generate the place your scenes happen in. Scenes stage on top of it.",
    actionLabel: "Generate a world",
  },
  scene: {
    title: "Stage a scene",
    detail: "Pick characters, place them in the world, and describe the shot.",
    actionLabel: "Stage a scene",
  },
  resolve_protected_tbd: {
    title: "Resolve an open story question",
    detail: "A locked fact in your story still has an unanswered question. Answer it to unblock the next step.",
    actionLabel: "Resolve it",
  },
  cinema_compilation: {
    title: "Compile the final prompt",
    detail: "Cinery assembles the scene into one ready-to-generate video prompt, with every reference recorded.",
    actionLabel: "Compile",
  },
};

export function readinessCopy(stepId: string): StepCopy | null {
  return STEP_COPY[stepId] ?? null;
}
