import { useState } from "react";
import { RefreshCw } from "lucide-react";
import { useSettings } from "../../query/downloadQueries";
import { useUpdateChecker } from "../../hooks/useUpdateChecker";
import UpdateResult from "./UpdateResult";
import { t } from "../../i18n";
import { AppDialog } from "../ui/dialog";
import { Button } from "../ui/button";
import { Select } from "../ui/select";

interface AboutDialogProps {
  onClose: () => void;
  onDownloadUpdate: (url: string) => void;
}

export default function AboutDialog({ onClose, onDownloadUpdate }: AboutDialogProps) {
  const [proxyName, setProxyName] = useState("");
  const { settings: loadedSettings } = useSettings();
  const proxies = loadedSettings?.proxies ?? {};
  const { version, checkState, updateInfo, errorMsg, handleCheck } = useUpdateChecker(proxyName);

  return (
    <AppDialog title={t("about.title")} onClose={onClose}>
      <div className="flex flex-col gap-3 p-4 text-center">
        <div className="text-[16px] font-semibold">ProxyDownloadManager</div>
        <div className="text-[12px] text-muted-foreground">{t("about.version")} {version}</div>
        <div className="text-[12px] text-muted-foreground">{t("about.description")}</div>
        <div className="text-[12px] text-muted-foreground">{t("about.techStack")}</div>
        <div className="flex items-center gap-2">
          <Select className="flex-1" value={proxyName} onChange={(e) => setProxyName(e.target.value)}>
            <option value="">{t("about.noProxy")}</option>
            {Object.keys(proxies).map((name) => (
              <option key={name} value={name}>{name}</option>
            ))}
          </Select>
          <Button onClick={handleCheck} disabled={checkState === "checking"}>
            <RefreshCw className="h-3.5 w-3.5" /> {checkState === "checking" ? t("about.checking") : t("about.checkUpdate")}
          </Button>
        </div>
        {checkState === "done" && updateInfo && (
          <UpdateResult updateInfo={updateInfo} onDownload={(url) => { onDownloadUpdate(url); onClose(); }} />
        )}
        {checkState === "error" && (
          <div className="text-left text-[12px] text-destructive">{t("about.updateCheckFailed")}: {errorMsg}</div>
        )}
      </div>
    </AppDialog>
  );
}
