import { computed, onMounted, ref, watch } from "vue";
import { useData, useRoute } from "vitepress";
import type { DocsThemeConfig, Game } from "./types";

const key = "f1mm-docs-game";
const remembered = ref<string | null>(null);

export function useGame() {
  const { theme, frontmatter, site } = useData<DocsThemeConfig>();
  const route = useRoute();
  const games = computed(() => theme.value.games);

  const path = computed(() => {
    const base = site.value.base;
    const p = route.path.startsWith(base) ? `/${route.path.slice(base.length)}` : route.path;
    return p.replace(/\.html$/, "").replace(/\/index$/, "/");
  });

  const own = computed<Game | null>(() => {
    const all = games.value;
    const named = all.find((g) => g.id === frontmatter.value.game);
    if (named) return named;
    const prefixed = all.find((g) => g.prefix !== "/" && path.value.startsWith(g.prefix));
    if (prefixed) return prefixed;
    const shared = theme.value.shared.some((s) => (s.endsWith("/") ? path.value.startsWith(s) : path.value === s));
    if (shared) return null;
    return all.find((g) => g.prefix === "/") ?? all[0];
  });

  const game = computed(() => own.value ?? games.value.find((g) => g.id === remembered.value) ?? games.value[0]);

  function remember(id: string) {
    remembered.value = id;
    try {
      localStorage.setItem(key, id);
    } catch {}
  }

  onMounted(() => {
    if (remembered.value === null) {
      try {
        remembered.value = localStorage.getItem(key);
      } catch {}
    }
    watch(own, (g) => g && remember(g.id), { immediate: true });
  });

  return { games, game };
}
