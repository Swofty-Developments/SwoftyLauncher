import type { Game } from "../theme/types";
import { launcher, releases } from "./shared";

export default {
  id: "overcooked_2",
  name: "Overcooked 2",
  title: "Swofty Launcher for Overcooked 2",
  icon: "chef",
  accent: "#f5a74b",
  prefix: "/overcooked-2/",
  landing: "/overcooked-2/",
  docs: "/overcooked-2/guide/",
  store: "https://store.steampowered.com/app/728880/",
  disclaimer: "Not affiliated with Team17 or Ghost Town Games.",
  tabs: [
    {
      id: "users",
      text: "Users",
      icon: "book",
      groups: [
        {
          text: "Getting started",
          items: [
            { text: "Introduction", link: "/overcooked-2/guide/" },
            { text: "Playing with the launcher", link: "/overcooked-2/guide/playing" },
          ],
        },
        launcher,
        releases,
      ],
    },
    {
      id: "developers",
      text: "Developers",
      icon: "code",
      mono: true,
      groups: [
        {
          text: "Start here",
          items: [
            { text: "Your first addon", link: "/overcooked-2/addons/", icon: "file" },
            { text: "The template", link: "/overcooked-2/addons/template", icon: "file" },
            { text: "swofty.json", link: "/overcooked-2/addons/manifest", icon: "file" },
            { text: "Building against the std lib", link: "/overcooked-2/addons/building", icon: "file" },
            { text: "How the core loads", link: "/overcooked-2/core", icon: "file" },
          ],
        },
        {
          text: "The standard library",
          items: [
            { text: "Addons and IAddon", link: "/overcooked-2/std/", icon: "file" },
            { text: "AddonContext and Log", link: "/overcooked-2/std/addon-context", icon: "file" },
            { text: "MainMenu", link: "/overcooked-2/std/main-menu", icon: "file" },
            { text: "Dialogs", link: "/overcooked-2/std/dialogs", icon: "file" },
            { text: "Game", link: "/overcooked-2/std/game", icon: "file" },
            { text: "Runtime", link: "/overcooked-2/std/runtime", icon: "file" },
            { text: "Harmony patches", link: "/overcooked-2/std/harmony", icon: "file" },
          ],
        },
      ],
    },
  ],
} satisfies Game;
