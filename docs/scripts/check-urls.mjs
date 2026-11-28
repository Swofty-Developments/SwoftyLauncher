#!/usr/bin/env node
// Fails when a URL the docs served before is gone after a change.
//
//   node docs/scripts/check-urls.mjs --before origin/main          build that ref in a temporary worktree first
//   node docs/scripts/check-urls.mjs --before <dist or list file>  compare against a build or a saved list
//   node docs/scripts/check-urls.mjs --list <dist>                 print the URLs of a build
//
// --after <dist> is the build to check, docs/.vitepress/dist by default (run pnpm docs:build first).
// --save <file> writes the after list.
//
// A URL is every file of the build outside assets/, as the site serves it: guide/index.html is /guide/,
// guide/linux.html is /guide/linux. A redirect page counts when the page it points at is in the build.
// It also fails on a link in a page of the after build, such as a sidebar entry, to a URL that build lacks.

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const docsDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const repoDir = path.dirname(docsDir);

function args(argv) {
  const out = {};
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (!a.startsWith("--")) throw new Error(`unexpected argument ${a}`);
    out[a.slice(2)] = argv[++i];
  }
  return out;
}

function walk(dir, rel = "") {
  return fs.readdirSync(path.join(dir, rel), { withFileTypes: true }).flatMap((e) => {
    const p = rel ? `${rel}/${e.name}` : e.name;
    if (e.isDirectory()) return p === "assets" ? [] : walk(dir, p);
    return [p];
  });
}

function urlOf(file) {
  if (file === "index.html") return "/";
  if (file.endsWith("/index.html")) return `/${file.slice(0, -"index.html".length)}`;
  if (file.endsWith(".html")) return `/${file.slice(0, -".html".length)}`;
  return `/${file}`;
}

function redirectOf(dist, file) {
  if (!file.endsWith(".html")) return null;
  const html = fs.readFileSync(path.join(dist, file), "utf8");
  const m = /<meta http-equiv="refresh" content="0; url=([^"]+)">/.exec(html);
  if (!m) return null;
  const to = m[1].replace(/^https?:\/\/[^/]+/, "").replace(/#.*$/, "");
  return to.endsWith(".html") ? to.slice(0, -".html".length) : to;
}

function linksOf(dist, file) {
  if (!file.endsWith(".html")) return [];
  const html = fs.readFileSync(path.join(dist, file), "utf8");
  const out = new Set();
  for (const m of html.matchAll(/href="(\/[^"#?]*)/g)) {
    const link = m[1];
    if (link.startsWith("//") || link.startsWith("/assets/")) continue;
    out.add(link.endsWith(".html") ? link.slice(0, -".html".length) : link);
  }
  return [...out];
}

function listBuild(dist) {
  if (!fs.existsSync(path.join(dist, "index.html"))) throw new Error(`${dist} is not a docs build`);
  const files = walk(dist);
  const urls = new Set(files.map(urlOf));
  const broken = [];
  const dead = new Map();
  for (const f of files) {
    const to = redirectOf(dist, f);
    if (to !== null && !urls.has(to)) broken.push(`${urlOf(f)} -> ${to}`);
    for (const link of linksOf(dist, f)) {
      if (urls.has(link) || urls.has(`${link}/`)) continue;
      if (!dead.has(link)) dead.set(link, urlOf(f));
    }
  }
  return { urls, broken, dead };
}

function readList(file) {
  return new Set(
    fs
      .readFileSync(file, "utf8")
      .split("\n")
      .map((l) => l.trim())
      .filter(Boolean),
  );
}

function buildRef(ref) {
  const work = fs.mkdtempSync(path.join(os.tmpdir(), "docs-urls-"));
  const tree = path.join(work, "tree");
  const run = (cmd, argv, cwd) => execFileSync(cmd, argv, { cwd, stdio: ["ignore", "inherit", "inherit"] });
  console.error(`building ${ref} in ${tree}`);
  run("git", ["worktree", "add", "--detach", tree, ref], repoDir);
  try {
    run("pnpm", ["install", "--frozen-lockfile", "--prefer-offline", "--ignore-scripts"], tree);
    run("pnpm", ["--filter", "@f1modmanager/docs", "docs:build"], tree);
    const dist = path.join(work, "dist");
    fs.cpSync(path.join(tree, "docs", ".vitepress", "dist"), dist, { recursive: true });
    return dist;
  } finally {
    run("git", ["worktree", "remove", "--force", tree], repoDir);
  }
}

function load(spec) {
  if (fs.existsSync(spec) && fs.statSync(spec).isFile()) return readList(spec);
  const dist = fs.existsSync(spec) ? spec : buildRef(spec);
  return listBuild(dist).urls;
}

const opts = args(process.argv.slice(2));
const sorted = (set) => [...set].sort();

if (opts.list) {
  process.stdout.write(`${sorted(listBuild(opts.list).urls).join("\n")}\n`);
  process.exit(0);
}

if (!opts.before) {
  console.error("usage: check-urls.mjs --before <ref, dist or list> [--after <dist>] [--save <file>]");
  process.exit(2);
}

const before = load(opts.before);
const after = listBuild(opts.after ?? path.join(docsDir, ".vitepress", "dist"));
if (opts.save) fs.writeFileSync(opts.save, `${sorted(after.urls).join("\n")}\n`);

const lost = sorted(before).filter((u) => !after.urls.has(u));
const added = sorted(after.urls).filter((u) => !before.has(u));

console.log(`${before.size} URLs before, ${after.urls.size} after, ${added.length} new, ${lost.length} lost`);
for (const u of added) console.log(`  new   ${u}`);
for (const u of lost) console.log(`  lost  ${u}`);
for (const b of after.broken) console.log(`  dead redirect  ${b}`);
for (const [link, from] of after.dead) console.log(`  dead link  ${link} (first in ${from})`);
process.exit(lost.length || after.broken.length || after.dead.size ? 1 : 0);
