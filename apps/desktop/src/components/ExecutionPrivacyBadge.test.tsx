import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { ExecutionPrivacyBadge, PrivacyBadgeDisplay } from "./ExecutionPrivacyBadge";

describe("ExecutionPrivacyBadge", () => {
  it("displays LOCAL badge for local execution", () => {
    render(<ExecutionPrivacyBadge location="local" />);
    expect(screen.getByText("LOCAL")).toBeInTheDocument();
  });

  it("displays CLOUD badge with provider for cloud execution", () => {
    render(<ExecutionPrivacyBadge location="cloud:openai" />);
    expect(screen.getByText("CLOUD: openai")).toBeInTheDocument();
  });

  it("displays provider ID when provided", () => {
    render(
      <ExecutionPrivacyBadge location="local" providerId="mock" />,
    );
    expect(screen.getByText("mock")).toBeInTheDocument();
  });

  it("displays model ID when provided", () => {
    render(
      <ExecutionPrivacyBadge location="local" providerId="mock" modelId="mock-image-v1" />,
    );
    expect(screen.getByText("mock-image-v1")).toBeInTheDocument();
  });

  it("applies custom className", () => {
    const { container } = render(
      <ExecutionPrivacyBadge location="local" className="custom-class" />,
    );
    expect(container.querySelector(".custom-class")).toBeInTheDocument();
  });
});

describe("PrivacyBadgeDisplay", () => {
  it("renders nothing when location is not provided", () => {
    const { container } = render(<PrivacyBadgeDisplay />);
    expect(container.firstChild).toBeNull();
  });

  it("normalizes cloud location format", () => {
    render(<PrivacyBadgeDisplay location="cloud:openai" />);
    expect(screen.getByText("CLOUD: openai")).toBeInTheDocument();
  });

  it("normalizes provider name without cloud prefix", () => {
    render(<PrivacyBadgeDisplay location="openai" />);
    expect(screen.getByText("CLOUD: openai")).toBeInTheDocument();
  });

  it("renders before and after content", () => {
    render(
      <PrivacyBadgeDisplay
        location="local"
        before={<span>Before:</span>}
        after={<span>After</span>}
      />,
    );
    expect(screen.getByText("Before:")).toBeInTheDocument();
    expect(screen.getByText("LOCAL")).toBeInTheDocument();
    expect(screen.getByText("After")).toBeInTheDocument();
  });
});
