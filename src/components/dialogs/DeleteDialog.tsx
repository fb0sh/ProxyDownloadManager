import { Button, Text } from "@primer/react";
import { Dialog } from "@primer/react/experimental";
import { useDeleteDownload } from "../../query/downloadQueries";

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
    <Dialog title="Delete Download" onClose={onClose}>
      <div style={{ padding: 16, display: "flex", flexDirection: "column", gap: 16 }}>
        <Text>
          Delete {ids.length} download{ids.length > 1 ? "s" : ""}?
        </Text>
        <div style={{ display: "flex", justifyContent: "flex-end", gap: 8 }}>
          <Button onClick={onClose}>Cancel</Button>
          <Button onClick={() => handleDelete(false)}>
            Delete Record Only
          </Button>
          <Button variant="danger" onClick={() => handleDelete(true)}>
            Delete File & Record
          </Button>
        </div>
      </div>
    </Dialog>
  );
}
