/**
 * Joins a project root path with a path stored relative to that root (e.g.
 * an `AssetVersion.thumbnailPath`) into a single filesystem path.
 *
 * Kept separate from JSX so no component concatenates filesystem paths
 * directly - callers pass the result through `convertFileSrc` (from
 * `@tauri-apps/api/core`) to obtain a URL usable in an `<img src>`.
 */
export function joinProjectRelativePath(
  projectRootPath: string,
  relativePath: string,
): string {
  const normalizedRoot = projectRootPath.replace(/[\\/]+$/, "");
  const normalizedRelative = relativePath.replace(/^[\\/]+/, "");
  return `${normalizedRoot}/${normalizedRelative}`;
}
