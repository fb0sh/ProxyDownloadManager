import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import App from "./App";
import NewDownloadWindow from "./NewDownloadWindow";
import DownloadDetailsWindow from "./DownloadDetailsWindow";

import "@primer/primitives/dist/css/functional/themes/light.css";
import { BaseStyles, ThemeProvider } from "@primer/react";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 1000,
      retry: 1,
    },
  },
});

function RootLayout() {
  // If opened as a child window for New Download, render standalone view
  const params = new URLSearchParams(window.location.search);
  const view = params.get("view");

  if (view === "new-download") {
    return (
      <QueryClientProvider client={queryClient}>
        <ThemeProvider>
          <BaseStyles>
            <NewDownloadWindow />
          </BaseStyles>
        </ThemeProvider>
      </QueryClientProvider>
    );
  }

  if (view === "download-details") {
    return (
      <QueryClientProvider client={queryClient}>
        <ThemeProvider>
          <BaseStyles>
            <DownloadDetailsWindow />
          </BaseStyles>
        </ThemeProvider>
      </QueryClientProvider>
    );
  }

  return (
    <QueryClientProvider client={queryClient}>
      <ThemeProvider>
        <BaseStyles>
          <App />
        </BaseStyles>
      </ThemeProvider>
    </QueryClientProvider>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <RootLayout />
  </React.StrictMode>,
);
