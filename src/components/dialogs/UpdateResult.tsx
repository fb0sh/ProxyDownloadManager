import { Text, Button } from "@primer/react";
import {
  CheckIcon,
  DownloadIcon,
  LinkExternalIcon,
  ArrowRightIcon,
} from "@primer/octicons-react";
import { t } from "../../i18n";
import type { UpdateInfo } from "../../types";

interface UpdateResultProps {
  updateInfo: UpdateInfo;
  onDownload: (url: string) => void;
}

export default function UpdateResult({ updateInfo, onDownload }: UpdateResultProps) {
  const recommendedAsset = updateInfo.assets.find((a) => a.recommended);
  const otherAssets = updateInfo.assets.filter((a) => !a.recommended);

  if (!updateInfo.has_update) {
    return (
      <div
        style={{
          padding: "8px 12px",
          borderRadius: 6,
          backgroundColor: "var(--success-bgColor, #dafbe1)",
          border: "1px solid var(--success-borderColor, #a6d3a0)",
          fontSize: 13,
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
          <CheckIcon size={16} />
          <Text>{t("about.upToDate")}</Text>
        </div>
      </div>
    );
  }

  return (
    <div
      style={{
        padding: "8px 12px",
        borderRadius: 6,
        backgroundColor: "var(--attention-bgColor, #fff8c5)",
        border: "1px solid var(--attention-borderColor, #d4a72c)",
        fontSize: 13,
      }}
    >
      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        <Text>
          {t("about.newVersion")}: <strong>{updateInfo.latest_version}</strong>
          <span style={{ margin: "0 4px", display: "inline-flex", verticalAlign: "middle" }}>
            <ArrowRightIcon size={12} />
          </span>
          <Text size="small" style={{ color: "var(--fgColor-muted, #656d76)" }}>
            {t("about.version")} {updateInfo.current_version}
          </Text>
        </Text>

        {recommendedAsset && (
          <Button
            size="small"
            variant="primary"
            leadingVisual={DownloadIcon}
            onClick={() => onDownload(recommendedAsset.url)}
          >
            {t("about.downloadUpdate")} ({recommendedAsset.name})
          </Button>
        )}

        {otherAssets.length > 0 && (
          <div style={{ display: "flex", flexWrap: "wrap", gap: 4 }}>
            {otherAssets.map((asset) => (
              <Button
                key={asset.name}
                size="small"
                leadingVisual={DownloadIcon}
                onClick={() => onDownload(asset.url)}
              >
                {asset.name}
              </Button>
            ))}
          </div>
        )}

        {updateInfo.release_notes && (
          <div>
            <Text weight="semibold" size="small" style={{ display: "block", marginBottom: 4 }}>
              {t("about.whatsNew")}
            </Text>
            <div
              style={{
                maxHeight: 200,
                overflow: "auto",
                padding: "8px 10px",
                borderRadius: 6,
                fontSize: 12,
                lineHeight: 1.6,
                whiteSpace: "pre-wrap",
                fontFamily: "ui-monospace, SFMono-Regular, SF Mono, Menlo, Consolas, monospace",
                backgroundColor: "var(--bgColor-muted, #f6f8fa)",
                border: "1px solid var(--borderColor-default, #d0d7de)",
              }}
            >
              {updateInfo.release_notes}
            </div>
          </div>
        )}

        <a
          href={updateInfo.release_url}
          target="_blank"
          rel="noreferrer"
          style={{ fontSize: 12, display: "inline-flex", alignItems: "center", gap: 4 }}
        >
          {t("about.releasePage")} <LinkExternalIcon size={12} />
        </a>
      </div>
    </div>
  );
}
