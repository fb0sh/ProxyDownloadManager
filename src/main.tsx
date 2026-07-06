import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import App from "./App";

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
