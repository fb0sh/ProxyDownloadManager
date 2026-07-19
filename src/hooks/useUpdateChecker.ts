import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import type { UpdateInfo } from "../types";

export type CheckState = "idle" | "checking" | "done" | "error";

export function useUpdateChecker(proxyName: string) {
  const [version, setVersion] = useState("");
  const [checkState, setCheckState] = useState<CheckState>("idle");
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [errorMsg, setErrorMsg] = useState("");

  useEffect(() => {
    getVersion().then(setVersion).catch(() => setVersion("?"));
  }, []);

  const handleCheck = async () => {
    setCheckState("checking");
    setErrorMsg("");
    try {
      const info = await invoke<UpdateInfo>("check_update", {
        proxyName,
      });
      setUpdateInfo(info);
      setCheckState("done");
    } catch (e) {
      setErrorMsg(String(e));
      setCheckState("error");
    }
  };

  return { version, checkState, updateInfo, errorMsg, handleCheck };
}
