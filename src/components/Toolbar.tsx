import { Button } from "@primer/react";
import { PlusIcon, TriangleRightIcon, StopIcon, TrashIcon, GearIcon, QuestionIcon, NoteIcon, BrowserIcon } from "@primer/octicons-react";
import { t } from "../i18n";
import { useAppContext } from "../contexts/AppContext";
import type { DownloadItem } from "../types";

interface ToolbarProps {
  hasDownloadingSelected: boolean;
  hasPausedSelected: boolean;
  hasCompletedSelected: boolean;
  hasFailedSelected: boolean;
  hasRedownloadable: boolean;
  onRedownloadItem?: DownloadItem;
}

export default function Toolbar({
  hasDownloadingSelected, hasPausedSelected, hasCompletedSelected, hasFailedSelected,
  hasRedownloadable, onRedownloadItem,
}: ToolbarProps) {
  const { actions } = useAppContext();
  const { onNewDownload, onExtension, onSettings, onAbout, onQuit, onLog,
    onResumeSelected, onPauseSelected, onDeleteSelected, onRedownload } = actions;

  const handleRedownloadSelected = onRedownloadItem ? () => onRedownload(onRedownloadItem) : undefined;

  return (
    <div
      style={{
        display: "flex",
        padding: "2px 6px",
        gap: 2,
        borderBottom: "1px solid var(--borderColor-muted, #d8dee4)",
        backgroundColor: "var(--bgColor-subtle, #f6f8fa)",
        alignItems: "center",
      }}
    >
      <Button onClick={onNewDownload} leadingVisual={PlusIcon} size="small" variant="primary">
        {t("toolbar.new")}
      </Button>
      {hasPausedSelected && (
        <Button onClick={onResumeSelected} leadingVisual={TriangleRightIcon} size="small">
          {t("toolbar.resume")}
        </Button>
      )}
      {hasDownloadingSelected && (
        <Button onClick={onPauseSelected} leadingVisual={StopIcon} size="small">
          {t("toolbar.stop")}
        </Button>
      )}
      {(hasPausedSelected || hasDownloadingSelected || hasCompletedSelected || hasFailedSelected) && (
        <Button onClick={onDeleteSelected} leadingVisual={TrashIcon} size="small" variant="danger">
          {t("toolbar.delete")}
        </Button>
      )}
      {hasRedownloadable && handleRedownloadSelected && (
        <Button onClick={handleRedownloadSelected} size="small">
          {t("toolbar.redownload")}
        </Button>
      )}
      <div style={{ flex: 1 }} />
      <Button onClick={onLog} leadingVisual={NoteIcon} size="small">
        {t("toolbar.log")}
      </Button>
      <Button onClick={onSettings} leadingVisual={GearIcon} size="small">
        {t("toolbar.settings")}
      </Button>
      <Button onClick={onExtension} leadingVisual={BrowserIcon} size="small">
        {t("toolbar.extension")}
      </Button>
      <Button onClick={onAbout} leadingVisual={QuestionIcon} size="small">
        {t("toolbar.about")}
      </Button>
      <Button onClick={onQuit} size="small">
        {t("toolbar.quit")}
      </Button>
    </div>
  );
}
