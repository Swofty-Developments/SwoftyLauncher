import type { DefaultTheme } from "vitepress";

export interface NavItem {
  text: string;
  link: string;
  icon?: string;
  linkText?: string;
  items?: NavItem[];
}

export interface NavGroup {
  text: string;
  items: NavItem[];
}

export interface NavTab {
  id: string;
  text: string;
  icon: string;
  mono?: boolean;
  groups: NavGroup[];
}

export interface Game {
  id: string;
  name: string;
  title: string;
  icon: string;
  accent: string;
  prefix: string;
  landing: string;
  docs: string;
  store?: string;
  disclaimer: string;
  tabs: NavTab[];
}

export interface DocsThemeConfig extends DefaultTheme.Config {
  repo: string;
  games: Game[];
  shared: string[];
}
