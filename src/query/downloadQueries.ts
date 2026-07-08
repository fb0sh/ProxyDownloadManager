// src/query/downloadQueries.ts
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useTauriCommands } from "../hooks/useTauriCommands";
import type { Settings } from "../types";

const DOWNLOADS_KEY = ["downloads"] as const;
const SETTINGS_KEY = ["settings"] as const;

export function useDownloads() {
  const { listDownloads } = useTauriCommands();
  return useQuery({
    queryKey: DOWNLOADS_KEY,
    queryFn: listDownloads,
    refetchInterval: 1000,
    refetchIntervalInBackground: true,
  });
}

export function useDownload(id: number | undefined) {
  const { data: downloads } = useDownloads();
  return downloads?.find((d) => d.id === id) ?? null;
}

export function useStartDownload() {
  const queryClient = useQueryClient();
  const { startDownload } = useTauriCommands();
  return useMutation({
    mutationFn: ({ url, filename, proxyName, connections, savePath }: {
      url: string; filename: string; proxyName: string; connections: number; savePath: string;
    }) => startDownload(url, filename, proxyName, connections, savePath),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: DOWNLOADS_KEY }),
  });
}

export function usePauseDownload() {
  const queryClient = useQueryClient();
  const { pauseDownload } = useTauriCommands();
  return useMutation({
    mutationFn: (id: number) => pauseDownload(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: DOWNLOADS_KEY }),
  });
}

export function useResumeDownload() {
  const queryClient = useQueryClient();
  const { resumeDownload } = useTauriCommands();
  return useMutation({
    mutationFn: (id: number) => resumeDownload(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: DOWNLOADS_KEY }),
  });
}

export function useDeleteDownload() {
  const queryClient = useQueryClient();
  const { deleteDownload } = useTauriCommands();
  return useMutation({
    mutationFn: ({ id, deleteFile }: { id: number; deleteFile: boolean }) =>
      deleteDownload(id, deleteFile),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: DOWNLOADS_KEY }),
  });
}

export function useSettings() {
  const { getSettings, saveSettings } = useTauriCommands();
  const queryClient = useQueryClient();

  const query = useQuery({
    queryKey: SETTINGS_KEY,
    queryFn: getSettings,
  });

  const mutation = useMutation({
    mutationFn: (settings: Settings) => saveSettings(settings),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: SETTINGS_KEY }),
  });

  return { settings: query.data, isLoading: query.isLoading, saveSettings: mutation.mutateAsync };
}

export function useRedownloadDownload() {
  const queryClient = useQueryClient();
  const { redownloadDownload } = useTauriCommands();
  return useMutation({
    mutationFn: (id: number) => redownloadDownload(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: DOWNLOADS_KEY }),
  });
}
