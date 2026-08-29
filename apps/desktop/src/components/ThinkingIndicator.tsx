import { ThinkingOrb } from "thinking-orbs";

const SIZE = 20;

export function ThinkingIndicator({ state = "working" }: { state?: "working" | "searching" | "solving" }) {
  return (
    <ThinkingOrb
      state={state}
      size={SIZE}
      theme="auto"
      aria-hidden={true}
    />
  );
}
