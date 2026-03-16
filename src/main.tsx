import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import App from "./App";
import { isEnglish } from "./lib/i18n";
import "./styles.css";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 5_000,
      refetchOnWindowFocus: false
    }
  }
});

if (typeof document !== "undefined") {
  document.documentElement.classList.toggle("locale-en-US", isEnglish);
}

type FatalState = {
  error: Error | null;
};

class RootErrorBoundary extends React.Component<React.PropsWithChildren, FatalState> {
  state: FatalState = {
    error: null
  };

  static getDerivedStateFromError(error: Error): FatalState {
    return { error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    console.error("Root render failed", error, info.componentStack);
  }

  render() {
    if (!this.state.error) {
      return this.props.children;
    }

    return (
      <div style={{
        padding: "24px",
        color: "#7f1d1d",
        whiteSpace: "pre-wrap",
        fontFamily: "\"Cascadia Mono\", Consolas, monospace"
      }}>
        <h2 style={{ marginTop: 0, fontFamily: "\"Microsoft YaHei UI\", \"Segoe UI\", sans-serif" }}>Application Error</h2>
        <div>{this.state.error.name}: {this.state.error.message}</div>
        {this.state.error.stack ? <pre>{this.state.error.stack}</pre> : null}
      </div>
    );
  }
}

function showFatalError(error: unknown) {
  const root = document.getElementById("root");
  if (!root) {
    return;
  }

  const normalized = error instanceof Error
    ? error
    : new Error(typeof error === "string" ? error : JSON.stringify(error));

  ReactDOM.createRoot(root).render(
    <div style={{
      padding: "24px",
      color: "#7f1d1d",
      whiteSpace: "pre-wrap",
      fontFamily: "\"Cascadia Mono\", Consolas, monospace"
    }}>
      <h2 style={{ marginTop: 0, fontFamily: "\"Microsoft YaHei UI\", \"Segoe UI\", sans-serif" }}>Startup Error</h2>
      <div>{normalized.name}: {normalized.message}</div>
      {normalized.stack ? <pre>{normalized.stack}</pre> : null}
    </div>
  );
}

window.addEventListener("error", (event) => {
  showFatalError(event.error ?? event.message);
});

window.addEventListener("unhandledrejection", (event) => {
  showFatalError(event.reason);
});

try {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <RootErrorBoundary>
        <QueryClientProvider client={queryClient}>
          <App />
        </QueryClientProvider>
      </RootErrorBoundary>
    </React.StrictMode>
  );
} catch (error) {
  showFatalError(error);
}
