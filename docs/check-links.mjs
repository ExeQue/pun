// Fails the docs build when an internal link in dist/ points at no page.
//
// Paths resolve the way the static host does: a last segment with a dot
// (`php-8.4`) is looked up as a file only, never as `php-8.4/index.html`.
// Also checks the rule links the RulesDirectory and MigrationTable islands
// build in the browser, which are not in the static HTML.
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const dist = new URL("./dist/", import.meta.url).pathname;
const site = "https://punphp.com";

const isFile = (path) => {
  try {
    return statSync(path).isFile();
  } catch {
    return false;
  }
};

function resolves(path) {
  const rel = decodeURIComponent(path).replace(/^\/+/, "");
  const target = join(dist, rel);
  if (path.endsWith("/")) return isFile(join(target, "index.html"));
  if (isFile(target)) return true;
  if (rel.split("/").pop().includes(".")) return false;
  return isFile(join(target, "index.html")) || isFile(`${target}.html`);
}

function* pages(dir) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) yield* pages(path);
    else if (entry.name.endsWith(".html")) yield path;
  }
}

const broken = new Map();
const report = (path, from) => {
  if (!resolves(path)) broken.set(path, [...(broken.get(path) ?? []), from]);
};

let count = 0;
for (const page of pages(dist)) {
  count++;
  const html = readFileSync(page, "utf8");
  for (const [, raw] of html.matchAll(/<(?:a|link)\b[^>]*?\shref="([^"]*)"/g)) {
    let href = raw.replaceAll("&amp;", "&");
    if (href.startsWith(site)) href = href.slice(site.length) || "/";
    if (!href.startsWith("/") || href.startsWith("//")) continue;
    report(href.split(/[?#]/)[0], `/${relative(dist, page)}`);
  }
}

// Island links: keep these in step with islands/RulesDirectory.tsx and MigrationTable.tsx.
const ruleUrl = (id) => `/docs/rules/${id.replaceAll(".", "-")}`;
const rules = JSON.parse(readFileSync(new URL("./generated/rules.json", import.meta.url)));
for (const rule of rules.rules) {
  report(ruleUrl(rule.id), "RulesDirectory");
  for (const name of rule.enabled) report(`/docs/rules/${name.includes("/") ? "sets" : "presets"}/${name}`, "RulesDirectory");
}
const migrations = JSON.parse(readFileSync(new URL("./generated/migrations.json", import.meta.url)));
for (const rows of Object.values(migrations.tools)) {
  for (const row of rows) for (const id of row.rules ?? []) report(ruleUrl(id), "MigrationTable");
}

for (const [path, from] of [...broken].sort()) {
  console.error(`404 ${path} (linked from ${from[0]}${from.length > 1 ? ` and ${from.length - 1} more` : ""})`);
}
console.log(`[check-links] ${count} pages, ${broken.size} broken link targets`);
process.exit(broken.size ? 1 : 0);
