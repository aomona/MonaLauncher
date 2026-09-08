import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles/index.css";

// Apply the saved appearance before React's first paint; storage can be unavailable.
try {
  const theme = localStorage.getItem("mona:theme");
  document.documentElement.dataset.theme = theme === "light" || theme === "dark" ? theme : "system";
} catch {
  document.documentElement.dataset.theme = "system";
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
