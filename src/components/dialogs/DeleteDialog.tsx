import { Button, Text } from "@primer/react";
import { Dialog } from "@primer/react/experimental";
import { useDeleteDownload } from "../../query/downloadQueries";
import { t } from "../../i18n";

interface DeleteDialogProps {
  ids: number[];
  onClose: () => void;
}

export default function DeleteDialog({ ids, onClose }: DeleteDialogProps) {
  const deleteDownload = useDeleteDownload();

  const handleDelete = async (deleteFile: boolean) => {
    await Promise.all(ids.map((id) => deleteDownload.mutateAsync({ id, deleteFile })));
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
