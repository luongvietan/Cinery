# Joey Cinema sequence-first flow

## Purpose

Cinery supports the filmmaker as director of short cinematic sequences. AI is an always-available co-director, not an autonomous author: it offers context-aware ideas, continuity checks, and prompt help, but never spends credits, changes creative choices, or promotes media without an explicit user action.

The design follows the supplied Joey Cinema Director workflow: create a world and continuity kit first, plan concise shots, inspect the production prompt before generation, review takes deliberately, then extend only an accepted take.

## Primary journey

The product organizes work around individual sequences rather than planning an entire film in a single pass.

1. **Sequence Library** — The project home lists sequences and their state: Draft, Ready, Generating, Review, and Locked. It also exposes total credit use. The creator starts a new sequence.
2. **Director Brief** — The creator defines the intended beat, action, emotional energy, target duration, and credit cap. The brief is intentionally human-authored. The AI can identify missing decisions and offer a small set of ideas when asked or when the user signals they are stuck.
3. **World & Continuity Kit** — The creator creates/selects a scene plate before video work, then attaches character sheets, wardrobe/group reference, props, world plates, and platform element tags. This screen makes missing continuity anchors visible.
4. **Shot Plan & Prompt** — The creator works through a short, timeboxed shot plan. They select the generation model, capture family, camera direction, sound rules, and references. The complete Joey-format production prompt is inspectable and editable before generation.
5. **Generate & Review** — A generation creates a candidate take. The creator can compare candidates, identify why a candidate works or fails, request a targeted retry, and explicitly promote one candidate as the canonical take. Existing takes remain available for comparison.
6. **Extend & Edit** — The creator begins from a canonical take. Cinery analyzes the selected clip and carries spatial/identity locks into an explicit prequel or sequel continuation. After accepting an extension, the creator sends selected clips to editing.

## Screen shell and AI co-director

Each sequence screen has a primary creator canvas and a persistent right rail for the AI co-director.

The right rail is present through all six screens and receives the current sequence state, active screen, selected references, and the currently editable artifact (brief, shot, prompt, take, or extension). Its default content is concise:

- progress/checklist for the current stage;
- continuity warnings or prerequisites;
- up to three optional suggestions;
- a clear action that requires the user to apply any proposed change.

The main canvas remains the source of truth for creative decisions. The rail must not automatically write a brief, attach references, change a prompt, generate a take, or accept/reject a candidate.

## State model

A sequence advances only through explicit creator actions:

`Draft → Brief locked → References ready → Prompt approved → Generating → In review → Canonical take selected → Extended/Ready for edit`

The product may return to an earlier editable state without destroying later artifacts. For example, a targeted retry creates a new candidate while retaining previous takes; changing references invalidates prompt approval and makes that state clear before another generation.

## Preflight and credit controls

Before the Generate action, Cinery shows:

- selected model and generation settings;
- all attached references and their roles;
- full generated prompt in Joey Cinema Director structure;
- requested runtime and any relevant prompt constraints;
- estimated credit impact and sequence/project spend.

The app blocks progression when a required continuity anchor is missing or a known production rule is violated, such as an unsupported runtime. Blocking copy identifies the missing field and links directly to the corrective control. The user remains free to edit the brief and plan before approving generation.

## Review and recovery

Generation failures preserve the brief, references, prompt, settings, and already-created candidates. The recovery path offers a deliberate retry, with an optional stated correction; it never retries automatically or silently spends credit.

Candidate comparison retains old output. Promoting a canonical take is explicit and records a user-supplied reason when an override is necessary. If the canonical selection changes concurrently, Cinery reloads the current state and asks the user to choose again rather than assuming intent.

Extend Video is enabled only from the currently canonical take. The creator chooses whether the new direction is a prequel or sequel, reviews the continuation prompt and carried locks, and then initiates the new generation explicitly.

## Validation and tests

Acceptance coverage will include:

1. Completing the six-screen happy path from an empty sequence to an accepted extension.
2. Missing brief, scene plate, character/world reference, or unsupported runtime prevents generation with corrective guidance.
3. A failed or cancelled generation retains inputs and requires an explicit retry.
4. Candidate comparison and canonical promotion preserve competing takes and handle stale selection conflicts.
5. An extension can only begin from the canonical take, carries continuity context, and requires a prequel/sequel choice.
6. The AI rail is visible at every stage, offers contextual suggestions, and cannot autonomously mutate sequence state or spend credits.

## Scope boundaries

This design covers the sequence-first flow and its persistent assistance model. It does not define a full-film planning workflow, autonomous agent execution, provider-specific pricing policy, or the editing application itself.
