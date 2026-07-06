import { Button } from "@primer/react";
import { PlusIcon, TriangleRightIcon, StopIcon, TrashIcon, GearIcon, QuestionIcon, NoteIcon } from "@primer/octicons-react";
import { t } from "../i18n";

interface ToolbarProps {
  onNewDownload: () => void;
  onSettings: () => void;
  onAbout: () => void;
  onQuit: () => void;
  onLog: () => void;
  onResumeSelected?: () => void;
  onPauseSelected?: () => void;
  onDeleteSelected?: () => void;
  onRedownloadSelected?: () => void;
  hasSelection: boolean;
  hasRedownloadable: boolean;
}

export default function Toolbar({
  onNewDownload, onSettings, onAbout, onQuit, onLog,
  onResumeSelected, onPauseSelected, onDeleteSelected, onRedownloadSelected,
  hasSelection, hasRedownloadable,
}: ToolbarProps) {
  return (
    <div
      style={{
        display: "flex",
        padding: "4px 8px",
        gap: 4,
        borderBottom: "1px solid var(--borderColor-muted, #d8dee4)",
        backgroundColor: "var(--bgColor-subtle, #f6f8fa)",
        alignItems: "center",
      }}
    >
      <Button onClick={onNewDownload} leadingVisual={PlusIcon} size="small" variant="primary">
        {t("toolbar.new")}
      </Button>
      {hasSelection && (
        <>
          <Button onClick={onResumeSelected} leadingVisual={TriangleRightIcon} size="small">
            {t("toolbar.resume")}
          </Button>
          <Button onClick={onPauseSelected} leadingVisual={StopIcon} size="small">
            {t("toolbar.stop")}
          </Button>
          <Button onClick={onDeleteSelected} leadingVisual={TrashIcon} size="small" variant="danger">
            {t("toolbar.delete")}
          </Button>
        </>
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
      <Button onClick={onAbout} leadingVisual={QuestionIcon} size="small">
        {t("toolbar.about")}
      </Button>
      <Button onClick={onQuit} size="small">
        {t("toolbar.quit")}
      </Button>
    </div>
  );
}
