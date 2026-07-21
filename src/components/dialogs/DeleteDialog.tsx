import { Button, Text } from "@primer/react";
import { Dialog } from "@primer/react/experimental";
import { useDeleteDownload } from "../../query/downloadQueries";
import { useAppContext } from "../../contexts/AppContext";
import { t } from "../../i18n";

interface DeleteDialogProps {
  ids: number[];
  onClose: () => void;
}

export default function DeleteDialog({ ids, onClose }: DeleteDialogProps) {
  console.debug('[ProxyDM FE] DeleteDialog mount ids=', ids);
  const deleteDownload = useDeleteDownload();
  const { selectionActions } = useAppContext();

  const handleDelete = async (deleteFile: boolean) => {
    await Promise.all(ids.map((id) => deleteDownload.mutateAsync({ id, deleteFile })));
    // Drop deleted rows only; leave any remaining multi-select intact
    selectionActions.removeIds(ids);
    onClose();
  };

  return (
    <Dialog title={t("delete.title")} onClose={onClose}>
      <div style={{ padding: "10px 12px", display: "flex", flexDirection: "column", gap: 10 }}>
        <Text size="small">
          {ids.length === 1 ? t("delete.confirm") : t("delete.confirmMultiple").replace("{count}", String(ids.length))}
        </Text>
        <div style={{ display: "flex", justifyContent: "flex-end", gap: 8 }}>
          <Button onClick={onClose}>{t("delete.cancel")}</Button>
          <Button onClick={() => handleDelete(false)}>
            {t("delete.delete")}
          </Button>
          <Button variant="danger" onClick={() => handleDelete(true)}>
            {t("delete.deleteFile")}
          </Button>
        </div>
      </div>
    </Dialog>
  );
}
