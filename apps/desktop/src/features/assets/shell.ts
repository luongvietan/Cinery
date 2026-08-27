import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import { joinProjectRelativePath } from "./paths";

export async function openProjectRelativePath(
  projectRootPath: string,
  relativePath: string,
): Promise<void> {
  await openPath(joinProjectRelativePath(projectRootPath, relativePath));
}

export async function revealProjectRelativePath(
  projectRootPath: string,
  relativePath: string,
): Promise<void> {
  await revealItemInDir(joinProjectRelativePath(projectRootPath, relativePath));
}

export async function openAssetFolder(
  projectRootPath: string,
  assetId: string,
): Promise<void> {
  await openPath(joinProjectRelativePath(projectRootPath, `assets/${assetId}`));
}
