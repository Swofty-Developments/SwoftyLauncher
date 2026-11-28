// Each game is a module in this folder: its id, title, path prefix, landing, docs home, accent, icon and tabs. Its
// pages live under its prefix (docs/overcooked-2/ for "/overcooked-2/"). Add a game by writing its module and listing
// it in games. F1 25 has the prefix "/" and keeps the site's original paths; shared.ts lists the pages every game shows.

import type { Game } from "../theme/types";
import f1_25 from "./f1_25";
import overcooked_2 from "./overcooked_2";

export { shared } from "./shared";

export const games: Game[] = [f1_25, overcooked_2];

export function gameOf(relativePath: string, frontmatterGame?: string) {
  const path = `/${relativePath.replace(/(^|\/)index\.md$/, "$1").replace(/\.md$/, "")}`;
  return (
    games.find((g) => g.id === frontmatterGame) ??
    games.find((g) => g.prefix !== "/" && path.startsWith(g.prefix)) ??
    games.find((g) => g.prefix === "/")!
  );
}
