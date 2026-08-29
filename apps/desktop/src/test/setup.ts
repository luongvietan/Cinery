import "@testing-library/jest-dom/vitest";
import { afterEach, beforeAll } from "vitest";
import { cleanup } from "@testing-library/react";

beforeAll(() => {
  // jsdom lacks the browser APIs liquid-gooey / thinking-orbs rely on.
  // Minimal no-op polyfills: components render, engines stay idle.
  if (typeof globalThis.ResizeObserver === "undefined") {
    class ResizeObserverPolyfill {
      observe() {}
      unobserve() {}
      disconnect() {}
    }
    globalThis.ResizeObserver = ResizeObserverPolyfill as unknown as typeof ResizeObserver;
  }
  if (typeof globalThis.IntersectionObserver === "undefined") {
    class IntersectionObserverPolyfill {
      root = null;
      rootMargin = "";
      thresholds = [];
      observe() {}
      unobserve() {}
      disconnect() {}
      takeRecords() {
        return [];
      }
    }
    globalThis.IntersectionObserver =
      IntersectionObserverPolyfill as unknown as typeof IntersectionObserver;
  }
  if (typeof globalThis.requestAnimationFrame === "undefined") {
    globalThis.requestAnimationFrame = ((callback: FrameRequestCallback) =>
      setTimeout(() => callback(performance.now()), 0) as unknown as number) as typeof requestAnimationFrame;
    globalThis.cancelAnimationFrame = ((handle: number) =>
      clearTimeout(handle as unknown as ReturnType<typeof setTimeout>)) as typeof cancelAnimationFrame;
  }
  if (typeof globalThis.matchMedia === "undefined") {
    Object.defineProperty(globalThis, "matchMedia", {
      writable: true,
      configurable: true,
      value: (query: string) => ({
        matches: false,
        media: query,
        onchange: null,
        addListener() {},
        removeListener() {},
        addEventListener() {},
        removeEventListener() {},
        dispatchEvent() {
          return false;
        },
      }),
    });
  }
  if (typeof globalThis.scrollTo === "undefined") {
    globalThis.scrollTo = (() => {}) as typeof scrollTo;
  }
  // jsdom emits "Not implemented: HTMLCanvasElement.prototype.getContext" on
  // every canvas mount (ThinkingOrb). The component already handles a null
  // context by rendering statically; silence the virtual-console noise.
interface PatchedCanvasProto {
  __getContextPatched?: boolean;
}

const canvasProto = globalThis.HTMLCanvasElement?.prototype as
  | (HTMLCanvasElement & PatchedCanvasProto)
  | undefined;
  if (canvasProto && !canvasProto.__getContextPatched) {
    Object.defineProperty(canvasProto, "getContext", {
      configurable: true,
      writable: true,
      value: () => null,
    });
    canvasProto.__getContextPatched = true;
  }
});

afterEach(() => {
  cleanup();
});
