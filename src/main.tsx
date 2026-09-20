import React from "react";
import ReactDOM from "react-dom/client";
import * as Sentry from "@sentry/react";
import App from "./App";
import { initializeSentry } from "./sentry";

initializeSentry();

function ErrorFallback({ resetError }: { resetError: () => void }) {
  return (
    <main className="fatal-error" role="alert">
      <div className="fatal-error__panel">
        <p className="fatal-error__eyebrow">Unexpected error</p>
        <h1>ClipFarmer couldn't load this screen.</h1>
        <p>The error was reported. You can retry without restarting the app.</p>
        <button type="button" onClick={resetError}>Try again</button>
      </div>
    </main>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Sentry.ErrorBoundary fallback={ErrorFallback}>
      <App />
    </Sentry.ErrorBoundary>
  </React.StrictMode>,
);
