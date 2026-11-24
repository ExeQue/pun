import { defineConfig } from "blume";
import { devAgentation } from "./dev-agentation.ts";
import { devModulePreload } from "./dev-modulepreload.ts";
import { ruleTables } from "./rule-tables.ts";

export default defineConfig({
  title: "Pun",
  description:
    "An all-in-one PHP toolchain in one Rust binary: format, lint, and analyse PHP 8.4+ projects and install their Composer packages, with one binary and one pun.toml.",
  logo: { image: "/logo.svg", text: "" },

  content: {
    root: "content",
  },

  deployment: {
    site: "https://pun.laravel.cloud",
  },

  integrations: [devModulePreload(), devAgentation(), ruleTables()],

  // The quickstart was merged into the introduction.
  redirects: [
    { from: "/docs/quickstart", to: "/docs#get-started" },
    { from: "/docs/why-three-commands", to: "/docs/vision" },
  ],

  github: {
    owner: "shipfastlabs",
    repo: "pun",
    dir: "docs",
  },

  lastModified: "git",

  navigation: {
    tabs: [{ label: "Docs", path: "/docs" }],
    actions: [{ href: "https://github.com/shipfastlabs/pun", label: "GitHub" }],
  },

  theme: {
    accent: { light: "#5B3FD9", dark: "#B3A3FF" },
    action: "#FFE53D",
    background: { light: "#FBF6EC", dark: "#120E18" },
    radius: "lg",
    mode: "system",
    fonts: {
      display: { name: "Fredoka", weights: ["300..700"] },
      body: { name: "Fredoka", weights: ["300..700"] },
      mono: { name: "Martian Mono", weights: [400, 500, 600, 700] },
    },
  },

  footer: {
    copyright: `© ${new Date().getFullYear()} Pun. MIT licensed.`,
    links: [
      { label: "Docs", href: "/docs" },
      { label: "Report an issue", href: "https://github.com/shipfastlabs/pun/issues" },
    ],
  },

  agents: {
    llmsTxt: {
      details:
        "Pun is a Rust toolchain for PHP 8.4+ projects. It reads pun.toml. `pun format` changes layout; `pun lint --fix` applies code fixes; `pun analyse` reports type errors; `pun check` runs all three, with --fix for format and lint edits. `pun migrate` converts existing tool configs and reports skipped items. `pun install`, `update`, `require` and `remove` read composer.json and write the composer.lock and vendor/ Composer writes; `pun x` runs a package's command without adding it; PHPStan, Larastan, and Rector support is incomplete; Planned rules are not implemented. Blade formatting requires Node.js and Prettier. Coding agents receive compact output automatically.",
    },
  },

  seo: {
    organization: {
      name: "Pun",
      logo: "/app-icon.svg",
      sameAs: ["https://github.com/shipfastlabs/pun"],
    },
  },
});
