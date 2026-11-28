import type { Game } from "../theme/types";
import f1_25 from "./f1_25";

export { shared } from "./shared";

export const games: Game[] = [f1_25];

export function gameOf(relativePath: string, frontmatterGame?: string) {
  const path = `/${relativePath.replace(/(^|\/)index\.md$/, "$1").replace(/\.md$/, "")}`;
  return (
    games.find((g) => g.id === frontmatterGame) ??
    games.find((g) => g.prefix !== "/" && path.startsWith(g.prefix)) ??
    games.find((g) => g.prefix === "/")!
  );
}
