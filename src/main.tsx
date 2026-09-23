import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import App from "./App";
import NewDownloadWindow from "./NewDownloadWindow";
import DownloadDetailsWindow from "./DownloadDetailsWindow";
import "./index.css";
import { tauriClient } from "./tauriClient";
import { setLanguage } from "./i18n";

tauriClient
  .getSettings()
  .then((s) => setLanguage(s.language || "en"))
  .catch(() => {});

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 1000,
      retry: 1,
    },
  },
});

function RootLayout() {
  const params = new URLSearchParams(window.location.search);
  const view = params.get("view");

  if (view === "new-download") {
    return (
      <QueryClientProvider client={queryClient}>
        <NewDownloadWindow />
      </QueryClientProvider>
    );
  }

  if (view === "download-details") {
    return (
      <QueryClientProvider client={queryClient}>
        <DownloadDetailsWindow />
      </QueryClientProvider>
    );
  }

  return (
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <RootLayout />
  </React.StrictMode>,
);
