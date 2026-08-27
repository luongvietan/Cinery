import type {
  Asset,
  AssetVersion,
  AssetWithVersions,
  CreateAssetInput,
  ImportAssetVersionInput,
} from "@cinematic/domain";
import { invokeCommand } from "../../lib/tauri";

export function createAsset(input: CreateAssetInput): Promise<Asset> {
  return invokeCommand<Asset>("create_asset", { ...input });
}

export function importAssetVersion(
  input: ImportAssetVersionInput,
): Promise<AssetVersion> {
  return invokeCommand<AssetVersion>("import_asset_version", { ...input });
}

export function listAssets(projectRootPath: string): Promise<Asset[]> {
  return invokeCommand<Asset[]>("list_assets", { projectRootPath });
}

export function getAssetWithVersions(
  projectRootPath: string,
  assetId: string,
): Promise<AssetWithVersions> {
  return invokeCommand<AssetWithVersions>("get_asset_with_versions", {
    projectRootPath,
    assetId,
  });
}
