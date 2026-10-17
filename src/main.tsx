import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { reportError } from "./api";
import { ErrorBoundary } from "./components/common";
import "./styles.css";

// Errors outside React's render cycle are recorded locally too (redacted and rate-limited in Rust).
window.addEventListener("error", (e) => reportError(e.error ?? e.message));
window.addEventListener("unhandledrejection", (e) => reportError(e.reason));

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </React.StrictMode>,
);
