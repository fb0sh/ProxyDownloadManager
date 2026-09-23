import { t } from "../../i18n";
import type { UpdateInfo } from "../../types";
import { Button } from "../ui/button";

interface UpdateResultProps {
  updateInfo: UpdateInfo;
  onDownload: (url: string) => void;
}

export default function UpdateResult({ updateInfo, onDownload }: UpdateResultProps) {
  const recommendedAsset = updateInfo.assets.find((a) => a.recommended);
  const otherAssets = updateInfo.assets.filter((a) => !a.recommended);

  if (!updateInfo.has_update) {
    return <div className="rounded-md border border-success/40 bg-success/10 p-2 text-left text-[13px] text-success">{t("about.upToDate")}</div>;
  }

  return (
    <div className="flex flex-col gap-2 rounded-md border border-warning/40 bg-warning/10 p-3 text-left">
      <div className="text-[13px]">
        {t("about.version")} {updateInfo.current_version} → <strong>{updateInfo.latest_version}</strong>
      </div>
      {recommendedAsset && (
        <Button variant="default" onClick={() => onDownload(recommendedAsset.url)}>
          {t("about.downloadUpdate")} ({recommendedAsset.name})
        </Button>
      )}
      {otherAssets.map((asset) => (
        <Button key={asset.name} size="sm" onClick={() => onDownload(asset.url)}>{asset.name}</Button>
      ))}
      {updateInfo.release_notes && (
        <pre className="max-h-40 overflow-auto bg-muted p-2 font-mono text-[12px] whitespace-pre-wrap">{updateInfo.release_notes}</pre>
      )}
      <a className="text-[12px] text-primary" href={updateInfo.release_url} target="_blank" rel="noreferrer">{t("about.releasePage")}</a>
    </div>
  );
}
