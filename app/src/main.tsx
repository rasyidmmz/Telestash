import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./i18n";
import "@fontsource-variable/outfit";
import { formatLogValue, recordErrorLog } from "./errorLogs";

const originalConsoleError = console.error.bind(console);

console.error = (...args: unknown[]) => {
  recordErrorLog({
    source: "console.error",
    message: args.map(formatLogValue).join(" "),
    details: args.map(formatLogValue).join("\n\n"),
  });
  originalConsoleError(...args);
};

window.onerror = function (message, source, lineno, colno, error) {
  recordErrorLog({
    source: "window.onerror",
    message: String(message),
    details: [
      `${source || "unknown"}:${lineno || 0}:${colno || 0}`,
      error?.stack || formatLogValue(error),
    ].filter(Boolean).join("\n"),
  });
  originalConsoleError("Global JS Error:", message, "at", source, lineno + ":" + colno, error?.stack || error);
  return false;
};

window.addEventListener("unhandledrejection", function (event) {
  recordErrorLog({
    source: "unhandledrejection",
    message: formatLogValue(event.reason),
    details: event.reason?.stack || formatLogValue(event.reason),
  });
  originalConsoleError("Unhandled Promise Rejection:", event.reason, event.reason?.stack || event.reason);
});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);

