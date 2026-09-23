import { useDeleteDownload } from "../../query/downloadQueries";
import { useAppContext } from "../../contexts/AppContext";
import { t } from "../../i18n";
import { AppDialog } from "../ui/dialog";
import { Button } from "../ui/button";

interface DeleteDialogProps {
  ids: number[];
  onClose: () => void;
}

export default function DeleteDialog({ ids, onClose }: DeleteDialogProps) {
  const deleteDownload = useDeleteDownload();
  const { selectionActions } = useAppContext();

  const handleDelete = async (deleteFile: boolean) => {
    await Promise.all(ids.map((id) => deleteDownload.mutateAsync({ id, deleteFile })));
    selectionActions.removeIds(ids);
    onClose();
  };

  return (
    <AppDialog title={t("delete.title")} onClose={onClose}>
      <div className="flex flex-col gap-3 p-3">
        <p className="text-[13px]">
          {ids.length === 1 ? t("delete.confirm") : t("delete.confirmMultiple").replace("{count}", String(ids.length))}
        </p>
        <div className="flex justify-end gap-2">
          <Button onClick={onClose}>{t("delete.cancel")}</Button>
          <Button onClick={() => handleDelete(false)}>{t("delete.delete")}</Button>
          <Button variant="destructive" onClick={() => handleDelete(true)}>{t("delete.deleteFile")}</Button>
        </div>
      </div>
    </AppDialog>
  );
}
