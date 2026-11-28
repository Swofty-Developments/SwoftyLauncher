import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { defineConfigWithTheme } from "vitepress";
import { newsFeed } from "../scripts/news.mjs";
import { gameOf, games, shared } from "./games";
import { flashCodeTheme } from "./theme/code-theme";
import type { DocsThemeConfig } from "./theme/types";

const repo = "https://github.com/Swofty-Developments/SwoftyLauncher";
const base = process.env.DOCS_BASE ?? "/";
const site = "https://f1.swofty.net";

const fonts =
  "https://fonts.googleapis.com/css2?family=Open+Sans:wght@400;600;700&family=Poppins:wght@400;600;700&family=Quicksand:wght@600;700&family=Source+Code+Pro:wght@400;500;600&family=Varela+Round&display=swap";

const pickTheme = `try{var t=localStorage.getItem("f1mm-docs-theme");if(t==="dark"||t==="ocean"||t==="peach")document.documentElement.classList.add("flash-theme-"+t)}catch(e){}`;

const pickSidebar = `try{if(localStorage.getItem("f1mm-docs-sidebar")==="closed")document.documentElement.classList.add("flash-sidebar-closed")}catch(e){}`;

const moved: Record<string, string> = {
  "/tutorials/first-addon": "/reference/addons/first-addon",
  "/tutorials/addon-settings": "/reference/addons/settings",
  "/tutorials/hooking": "/reference/addons/hooking",
  "/tutorials/in-game-ui": "/reference/addons/in-game-ui",
  "/tutorials/database": "/reference/data/database",
  "/tutorials/erp-assets": "/reference/data/textures",
  "/modding/template": "/reference/addons/template",
  "/modding/manifest": "/reference/addons/manifest",
  "/modding/native-plugins": "/reference/addons/native-plugins",
  "/modding/database-ops": "/reference/ops/database",
  "/modding/erp-ops": "/reference/ops/erp",
  "/modding/ui-patches": "/reference/ops/ui-patches",
  "/reference/std/files": "/reference/ops/files",
  "/using/mods-and-maps": "/using/addons",
  "/using/importing-maps": "/using/mods",
  "/guide/addons-mods-maps": "/guide/addons-mods-tracks",
  "/reference/maps": "/reference/installing-tracks",
  "/guide/making-a-mod": "/reference/mods/",
  "/guide/making-an-addon": "/reference/addons/",
  "/reference/mods/database": "/reference/data/database",
  "/reference/mods/text": "/reference/data/text",
  "/reference/mods/erp-assets": "/reference/data/textures",
  "/reference/mods/livery": "/reference/data/livery",
  "/reference/mods/packaging": "/reference/addons/publishing",
  "/reference/mods/publishing": "/reference/addons/publishing",
  "/support/index": "/using/donate",
  "/support/thanks": "/using/donate",
};

export default defineConfigWithTheme<DocsThemeConfig>({
  title: "Swofty Launcher for F1 25",
  description: "Addons and mods for F1 25 on Windows and Linux",
  base,
  lang: "en-AU",
  appearance: "force-dark",
  srcExclude: ["**/public/**", "scripts/**"],
  sitemap: { hostname: site },
  buildEnd({ outDir }) {
    writeFileSync(join(outDir, "news.json"), `${JSON.stringify(newsFeed())}\n`);
    for (const [from, to] of Object.entries(moved)) {
      const url = to.endsWith("/") ? `${base}${to.slice(1)}` : `${base}${to.slice(1)}.html`;
      const file = join(outDir, `${from}.html`);
      mkdirSync(dirname(file), { recursive: true });
      writeFileSync(
        file,
        `<!doctype html><meta charset="utf-8"><title>Moved</title><link rel="canonical" href="${url}"><meta http-equiv="refresh" content="0; url=${url}"><script>location.replace(${JSON.stringify(url)} + location.hash)</script><a href="${url}">${url}</a>\n`,
      );
    }
  },
  head: [
    ["link", { rel: "icon", href: `${base}logo.svg`, type: "image/svg+xml" }],
    ["meta", { name: "theme-color", content: "#2a1b39" }],
    ["meta", { property: "og:site_name", content: "Swofty Launcher for F1 25" }],
    ["meta", { property: "og:image", content: `${site}/screenshots/hero-launcher.png` }],
    ["link", { rel: "preconnect", href: "https://fonts.googleapis.com" }],
    ["link", { rel: "preconnect", href: "https://fonts.gstatic.com", crossorigin: "" }],
    ["link", { rel: "stylesheet", href: fonts }],
    ["script", {}, pickTheme],
    ["script", {}, pickSidebar],
  ],
  markdown: {
    theme: flashCodeTheme,
    lineNumbers: true,
    config(md) {
      const { rules } = md.renderer;
      const code = rules.code_inline!;
      rules.table_open = (tokens, idx, options, env, self) => {
        env.inTable = true;
        return `<div class="table-wrap">${self.renderToken(tokens, idx, options)}`;
      };
      rules.table_close = (tokens, idx, options, env, self) => {
        env.inTable = false;
        return `${self.renderToken(tokens, idx, options)}</div>\n`;
      };
      const cell: typeof code = (tokens, idx, options, _env, self) => {
        const inline = tokens[idx + 1];
        if (inline?.type === "inline" && inline.content.length <= 20) tokens[idx].attrJoin("class", "short");
        return self.renderToken(tokens, idx, options);
      };
      rules.th_open = cell;
      rules.td_open = cell;
      rules.code_inline = (tokens, idx, options, env, self) => {
        if (!env.inTable) return code(tokens, idx, options, env, self);
        const t = tokens[idx];
        const body = md.utils.escapeHtml(t.content).replace(/\.(?=[A-Za-z_])/g, ".<wbr>");
        return `<code${self.renderAttrs(t)}>${body}</code>`;
      };
    },
  },
  transformPageData(pageData) {
    const game = gameOf(pageData.relativePath, pageData.frontmatter.game);
    if (game.prefix !== "/" && pageData.titleTemplate === undefined) pageData.titleTemplate = `:title | ${game.title}`;
  },
  themeConfig: {
    logo: "/logo.svg",
    siteTitle: "Swofty Launcher for F1 25",
    repo,
    socialLinks: [{ icon: "github", link: repo }],
    search: {
      provider: "local",
    },
    games,
    shared,
  },
});
