import { Button } from "@primer/react";
import { PlusIcon, TriangleRightIcon, StopIcon, TrashIcon, GearIcon, QuestionIcon, NoteIcon } from "@primer/octicons-react";

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
        padding: 8,
        gap: 8,
        borderBottom: "1px solid var(--borderColor-default, #d0d7de)",
        backgroundColor: "var(--bgColor-subtle, #f6f8fa)",
        alignItems: "center",
      }}
    >
      <Button onClick={onNewDownload} leadingVisual={PlusIcon} size="small" variant="primary">
        New
      </Button>
      {hasSelection && (
        <>
          <Button onClick={onResumeSelected} leadingVisual={TriangleRightIcon} size="small">
            Resume
          </Button>
          <Button onClick={onPauseSelected} leadingVisual={StopIcon} size="small">
            Stop
          </Button>
          <Button onClick={onDeleteSelected} leadingVisual={TrashIcon} size="small" variant="danger">
            Delete
          </Button>
        </>
      )}
      {hasRedownloadable && (
        <Button onClick={onRedownloadSelected} size="small">
          Redownload
        </Button>
      )}
      <div style={{ flex: 1 }} />
      <Button onClick={onLog} leadingVisual={NoteIcon} size="small">
        Log
      </Button>
      <Button onClick={onSettings} leadingVisual={GearIcon} size="small">
        Settings
      </Button>
      <Button onClick={onAbout} leadingVisual={QuestionIcon} size="small">
        About
      </Button>
      <Button onClick={onQuit} size="small">
        Quit
      </Button>
    </div>
  );
}
