import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./styles/theme.css";
import "./styles/tile.css";
import "./styles/lobby.css";
import "./styles/table.css";
import "./styles/animations.css";

const el = document.getElementById("root");
if (!el) throw new Error("root element missing");

createRoot(el).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
