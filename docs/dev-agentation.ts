import type { AstroIntegration } from "astro";

// Agentation (https://agentation.com) adds a toolbar for clicking any element and leaving a note, then copying
// all notes as Markdown for a coding agent. Dev server only: `blume build` never includes it.
export function devAgentation(): AstroIntegration {
  return {
    name: "pun-dev-agentation",
    hooks: {
      "astro:config:setup": ({ command, injectScript }) => {
        if (command !== "dev") return;
        injectScript(
          "page",
          `import { createElement } from "react";
import { createRoot } from "react-dom/client";
import { Agentation } from "agentation";
if (!document.getElementById("pun-agentation")) {
  const host = document.createElement("div");
  host.id = "pun-agentation";
  document.body.appendChild(host);
  createRoot(host).render(createElement(Agentation));
}`
        );
      },
    },
  };
}
