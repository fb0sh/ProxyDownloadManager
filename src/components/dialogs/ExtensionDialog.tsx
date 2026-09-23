import { useEffect, useState } from "react";
import { FolderOpen, Copy } from "lucide-react";
import { tauriClient } from "../../tauriClient";
import { t } from "../../i18n";
import { AppDialog } from "../ui/dialog";
import { Button } from "../ui/button";

interface ExtensionDialogProps {
  onClose: () => void;
}

export default function ExtensionDialog({ onClose }: ExtensionDialogProps) {
  const [extDir, setExtDir] = useState("");
  const [copyLabel, setCopyLabel] = useState(t("extension.copy"));

  useEffect(() => {
    tauriClient.getExtensionsDir().then(setExtDir).catch(() => setExtDir(""));
  }, []);

  return (
    <AppDialog title={t("toolbar.extension")} onClose={onClose}>
      <div className="flex flex-col gap-3 p-3 text-[13px]">
        <p>{t("extension.intro")}</p>
        <div className="flex gap-2">
          <Button onClick={() => tauriClient.openExtensionsFolder().then(onClose)}>
            <FolderOpen className="h-3.5 w-3.5" /> {t("extension.openFolder")}
          </Button>
          <Button onClick={async () => {
            await navigator.clipboard.writeText(extDir);
            setCopyLabel(t("extension.copied"));
          }}>
            <Copy className="h-3.5 w-3.5" /> {copyLabel}
          </Button>
        </div>
        <ol className="list-decimal pl-5 text-muted-foreground">
          <li>{t("extension.step1")}</li>
          <li>{t("extension.step2")}</li>
          <li>{t("extension.step3")}</li>
          <li>{t("extension.step4")}</li>
        </ol>
        <p className="font-semibold">{t("extension.firefox")}</p>
        <ol className="list-decimal pl-5 text-muted-foreground">
          <li>{t("extension.ffStep1")}</li>
          <li>{t("extension.ffStep2")}</li>
          <li>{t("extension.ffStep3")}</li>
        </ol>
        <p className="text-[12px] text-muted-foreground">{t("extension.ffNote")}</p>
      </div>
    </AppDialog>
  );
}
