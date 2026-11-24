import type { AstroIntegration } from "astro";

// Markdown tables full of rule IDs and upstream names (sets, presets, merged rules, rule options) clip or wrap
// mid-word, because a long ID has no break point the browser likes. On every page load this:
// - adds a <wbr> after ".", "/", "\", "_" and "-" in table code, and inside long camelCase names, so IDs break
//   only at those points;
// - copies each column's header onto its cells as data-label, so theme.css can stack wide tables as cards on phones.
// The migrate pages' MigrationTable island already does both, so its table is skipped.
const script = `
const SEPARATOR = /(?<=[./\\\\_-])/;
const CAMEL = /(?<=[a-z0-9])(?=[A-Z])/;
function breakable(code) {
  if (code.dataset.wbr || code.children.length) return;
  code.dataset.wbr = "1";
  const parts = code.textContent.split(SEPARATOR).flatMap(part => part.length > 24 ? part.split(CAMEL) : [part]);
  if (parts.length < 2) return;
  code.replaceChildren(...parts.flatMap((part, i) => i ? [document.createElement("wbr"), part] : [part]));
}
function enhance() {
  for (const table of document.querySelectorAll(".prose .blume-table-scroll > table")) {
    if (table.closest(".migration-table") || table.dataset.enhanced) continue;
    table.dataset.enhanced = "1";
    table.querySelectorAll("td code").forEach(breakable);
    const labels = [...table.querySelectorAll("thead th")].map(th => th.textContent.trim());
    if (labels.length < 3) continue;
    table.classList.add("pun-stack");
    for (const row of table.querySelectorAll("tbody tr")) {
      [...row.children].forEach((cell, i) => { if (labels[i]) cell.dataset.label = labels[i]; });
    }
  }
}
document.addEventListener("astro:page-load", enhance);
enhance();
`;

export function ruleTables(): AstroIntegration {
  return {
    name: "pun-rule-tables",
    hooks: {
      "astro:config:setup": ({ injectScript }) => injectScript("page", script),
    },
  };
}
