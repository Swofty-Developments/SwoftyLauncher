import { readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import type { NavGroup } from "../theme/types";

export const shared = [
  "/terms",
  "/privacy",
  "/dmca",
  "/news/",
  "/releases/",
  "/guide/installation",
  "/using/updating",
  "/using/settings",
  "/using/send-logs",
  "/using/news",
  "/using/donate",
];

const releasesDir = fileURLToPath(new URL("../../releases", import.meta.url));

function newestFirst(a: string, b: string) {
  const x = a.split(".").map(Number);
  const y = b.split(".").map(Number);
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    const d = (y[i] ?? 0) - (x[i] ?? 0);
    if (d) return d;
  }
  return 0;
}

export const releases: NavGroup = {
  text: "Releases",
  items: readdirSync(releasesDir)
    .filter((f) => /^\d+(\.\d+)*\.md$/.test(f))
    .map((f) => f.slice(0, -3))
    .sort(newestFirst)
    .map((v) => ({ text: v, link: `/releases/${v}` })),
};

export const launcher: NavGroup = {
  text: "The launcher",
  items: [
    { text: "Installation", link: "/guide/installation" },
    { text: "News", link: "/using/news" },
    { text: "Settings", link: "/using/settings" },
    { text: "Updating", link: "/using/updating" },
    { text: "Sending your logs", link: "/using/send-logs" },
    { text: "Donate", link: "/using/donate" },
  ],
};
