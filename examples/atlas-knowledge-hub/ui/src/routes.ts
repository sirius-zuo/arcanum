import {
  Activity,
  BookOpen,
  FlaskConical,
  GitFork,
  LayoutDashboard,
  MessageSquareText,
  Plug,
  Search,
  ShieldCheck,
  Layers,
  FileSearch,
} from 'lucide-react'
import type { LucideIcon } from 'lucide-react'

export interface RouteMeta {
  path: string
  label: string
  icon: LucideIcon
  blurb: string
}

/** Rail order, which is also the tour order (spec 7). */
export const ROUTES: RouteMeta[] = [
  { path: '/', label: 'Overview', icon: LayoutDashboard, blurb: 'Health, sample corpus and a map of what Atlas demonstrates.' },
  { path: '/library', label: 'Library', icon: BookOpen, blurb: 'Documents, versions and ingestion progress.' },
  { path: '/search', label: 'Search', icon: Search, blurb: 'Hybrid retrieval with the winning strategy per result.' },
  { path: '/context', label: 'Context', icon: Layers, blurb: 'Token-budgeted, citation-ready passages for your own LLM.' },
  { path: '/ask', label: 'Ask', icon: MessageSquareText, blurb: 'Grounded answers that stream with clickable citations.' },
  { path: '/verify', label: 'Verify', icon: ShieldCheck, blurb: 'Sentence-level verdicts with evidence from the source.' },
  { path: '/evidence', label: 'Evidence', icon: FileSearch, blurb: 'Trace any id back to the exact bytes it came from.' },
  { path: '/graph', label: 'Graph', icon: GitFork, blurb: 'The entity and relation graph extracted from the corpus.' },
  { path: '/lab', label: 'Lab', icon: FlaskConical, blurb: 'Retrieval evaluation against a golden set.' },
  { path: '/admin', label: 'Admin', icon: Activity, blurb: 'Metrics, collections and housekeeping.' },
  { path: '/connect', label: 'Connect', icon: Plug, blurb: 'Use Atlas from your own tools over REST and MCP.' },
]

export function routeMeta(path: string): RouteMeta {
  const found = ROUTES.find((r) => r.path === path)
  if (!found) throw new Error(`unknown route ${path}`)
  return found
}
