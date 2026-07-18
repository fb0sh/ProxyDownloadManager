import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { tauriClient } from "../tauriClient";
import type { Settings } from "../types";

const DOWNLOADS_KEY = ["downloads"] as const;
const SETTINGS_KEY = ["settings"] as const;

export function useDownloads() {
  return useQuery({
    queryKey: DOWNLOADS_KEY,
    queryFn: tauriClient.listDownloads,
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
  return useMutation({
    mutationFn: ({ url, filename, proxyName, connections, savePath }: {
      url: string; filename: string; proxyName: string; connections: number; savePath: string;
    }) => tauriClient.startDownload(url, filename, proxyName, connections, savePath),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: DOWNLOADS_KEY }),
  });
}

export function usePauseDownload() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: number) => tauriClient.pauseDownload(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: DOWNLOADS_KEY }),
  });
}

export function useResumeDownload() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: number) => tauriClient.resumeDownload(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: DOWNLOADS_KEY }),
  });
}

export function useDeleteDownload() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, deleteFile }: { id: number; deleteFile: boolean }) =>
      tauriClient.deleteDownload(id, deleteFile),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: DOWNLOADS_KEY }),
  });
}

export function useSettings() {
  const queryClient = useQueryClient();

  const query = useQuery({
    queryKey: SETTINGS_KEY,
    queryFn: tauriClient.getSettings,
  });

  const mutation = useMutation({
    mutationFn: (settings: Settings) => tauriClient.saveSettings(settings),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: SETTINGS_KEY }),
  });

  return { settings: query.data, isLoading: query.isLoading, saveSettings: mutation.mutateAsync };
}

export function useRedownloadDownload() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: number) => tauriClient.redownloadDownload(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: DOWNLOADS_KEY }),
  });
}
