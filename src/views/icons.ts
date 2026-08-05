import { createElement, type IconNode } from "lucide";
import type { GroupIcon } from "../api";

// The curated render set for group icons. Names must match src/views/icon-catalog.json;
// a name that drifts renders the default group glyph instead of a broken image.
import {
  Banknote,
  BarChart3,
  Bell,
  BookOpen,
  Bookmark,
  Briefcase,
  Bug,
  Building2,
  Calendar,
  Camera,
  Car,
  Clock,
  Coffee,
  CreditCard,
  Cross,
  Crown,
  Dumbbell,
  FileText,
  Film,
  Flag,
  Flame,
  Folder,
  FolderTree,
  Gamepad2,
  Gift,
  Globe,
  GraduationCap,
  Hammer,
  Headphones,
  Heart,
  House,
  Image,
  Inbox,
  Key,
  Leaf,
  Link,
  Lock,
  Mail,
  MessageCircle,
  Music,
  Network,
  Package,
  Paintbrush,
  Phone,
  Plane,
  Settings,
  Shield,
  ShoppingCart,
  Sparkles,
  Star,
  Tag,
  Terminal,
  TrendingUp,
  User,
  Users,
  Wrench,
  Zap,
} from "lucide";

export { ChevronDown, Ellipsis, FolderOpen, Info, Minus, Pencil, Plus, X, type IconNode } from "lucide";

/** Every icon is decorative: the control it sits in carries the accessible name. */
export function icon(node: IconNode): SVGElement {
  const svg = createElement(node);
  svg.classList.add("icon");
  svg.setAttribute("aria-hidden", "true");
  svg.setAttribute("focusable", "false");
  return svg;
}

const SYMBOLS: Record<string, IconNode> = {
  Banknote,
  BarChart3,
  Bell,
  BookOpen,
  Bookmark,
  Briefcase,
  Bug,
  Building2,
  Calendar,
  Camera,
  Car,
  Clock,
  Coffee,
  CreditCard,
  Cross,
  Crown,
  Dumbbell,
  FileText,
  Film,
  Flag,
  Flame,
  Folder,
  FolderTree,
  Gamepad2,
  Gift,
  Globe,
  GraduationCap,
  Hammer,
  Headphones,
  Heart,
  House,
  Image,
  Inbox,
  Key,
  Leaf,
  Link,
  Lock,
  Mail,
  MessageCircle,
  Music,
  Network,
  Package,
  Paintbrush,
  Phone,
  Plane,
  Settings,
  Shield,
  ShoppingCart,
  Sparkles,
  Star,
  Tag,
  Terminal,
  TrendingUp,
  User,
  Users,
  Wrench,
  Zap,
};

/** A group's icon as a DOM node: emoji text, lucide glyph, or the default group glyph. */
export function groupIcon(glyph: GroupIcon | null): Element {
  if (glyph?.emoji) {
    const emoji = document.createElement("span");
    emoji.className = "group-glyph group-glyph-emoji";
    emoji.setAttribute("aria-hidden", "true");
    emoji.textContent = glyph.emoji;
    return emoji;
  }
  if (glyph?.symbol) {
    const node = SYMBOLS[glyph.symbol] ?? FolderTree;
    const svg = icon(node);
    svg.classList.add("group-glyph");
    return svg;
  }
  const fallback = icon(FolderTree);
  fallback.classList.add("group-glyph", "group-glyph-default");
  return fallback;
}

/** Resolve a catalog symbol to its icon node for the picker grid. */
export function symbolNode(name: string): IconNode {
  return SYMBOLS[name] ?? FolderTree;
}
