import { Button } from "@primer/react";
import { PlusIcon, TriangleRightIcon, StopIcon, TrashIcon, GearIcon, QuestionIcon, NoteIcon, BrowserIcon } from "@primer/octicons-react";
import { t } from "../i18n";

interface ToolbarProps {
  onNewDownload: () => void;
  onExtension: () => void;
  onSettings: () => void;
  onAbout: () => void;
  onQuit: () => void;
  onLog: () => void;
  onResumeSelected?: () => void;
  onPauseSelected?: () => void;
  onDeleteSelected?: () => void;
  onRedownloadSelected?: () => void;
  hasDownloadingSelected: boolean;
  hasPausedSelected: boolean;
  hasCompletedSelected: boolean;
  hasFailedSelected: boolean;
  hasRedownloadable: boolean;
}

export default function Toolbar({
  onNewDownload, onExtension, onSettings, onAbout, onQuit, onLog,
  onResumeSelected, onPauseSelected, onDeleteSelected, onRedownloadSelected,
  hasDownloadingSelected, hasPausedSelected, hasCompletedSelected, hasFailedSelected, hasRedownloadable,
}: ToolbarProps) {
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
      {hasRedownloadable && (
        <Button onClick={onRedownloadSelected} size="small">
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
