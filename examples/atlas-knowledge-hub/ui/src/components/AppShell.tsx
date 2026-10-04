import { useMemo, useState } from 'react'
import { Outlet } from 'react-router-dom'
import { Rail } from './Rail'
import { TopBar } from './TopBar'
import { ErrorState } from './ErrorState'
import { CommandPalette } from './CommandPalette'
import type { PaletteAction } from './CommandPalette'
import { TourOverlay } from './TourOverlay'
import { Inspector, InspectorProvider } from './Inspector'
import { CommandActionsProvider, useCommandActions } from '../state/commandActions'
import { TourProvider } from '../state/tour'
import { OperationsProvider } from '../state/operations'
import { IngestEventsProvider } from '../state/ingestEvents'
import { useApplyUpdate, useHealth, useLibrary, useLoadSamples } from '../api/library'
import { loadBlockedReason } from './LoadCorpusButton'
import { useTheme } from '../state/theme'
import { useBootstrap } from '../state/bootstrap'
import { safeGet, safeSet } from '../lib/storage'

const KEY = 'atlas.rail.collapsed'

function initialCollapsed(): boolean {
  const stored = safeGet(KEY)
  if (stored === '1') return true
  if (stored === '0') return false
  return typeof window !== 'undefined' && window.innerWidth < 1280
}

/** Palette actions that need app state; routes are added by the palette itself. */
function AppCommandPalette() {
  const load = useLoadSamples()
  const update = useApplyUpdate()
  const { toggle } = useTheme()
  const { run } = useCommandActions()
  const health = useHealth().data
  const docs = useLibrary().data?.documents.length ?? 0
  // The same gates as the Overview buttons: not ready blocks both, an update needs a corpus.
  const loadBlocked = loadBlockedReason(health) ?? (load.isPending ? 'Loading' : undefined)
  const updateBlocked = !health?.ready ? 'Not ready' : docs === 0 ? 'Load the corpus first' : update.isPending ? 'Applying' : undefined
  const actions = useMemo<PaletteAction[]>(
    () => [
      { id: 'load', label: 'Load sample corpus', run: () => load.mutate(), disabledReason: loadBlocked },
      { id: 'update', label: 'Apply policy update', run: () => update.mutate(), disabledReason: updateBlocked },
      { id: 'theme', label: 'Toggle theme', run: toggle },
      { id: 'tour', label: 'Start tour', run: () => run('tour.start') },
    ],
    [load, update, toggle, run, loadBlocked, updateBlocked],
  )
  return <CommandPalette actions={actions} />
}

export function AppShell() {
  const [collapsed, setCollapsed] = useState(initialCollapsed)
  const { error, retry } = useBootstrap()

  const toggle = () =>
    setCollapsed((c) => {
      safeSet(KEY, c ? '0' : '1')
      return !c
    })

  return (
    <OperationsProvider>
    <IngestEventsProvider>
    <CommandActionsProvider>
    <TourProvider>
    <InspectorProvider>
    <div className="flex min-h-screen">
      <Rail collapsed={collapsed} onToggle={toggle} />
      <div className="min-w-0 flex-1">
        <TopBar />
        <TourOverlay />
        <main className="mx-auto w-full max-w-[1200px] px-8 py-10">
          {error ? (
            <ErrorState
              title="Atlas could not reach its server"
              message={error.message}
              fix="cargo run   # in examples/atlas-knowledge-hub, then reload"
              action={
                <button
                  type="button"
                  onClick={retry}
                  className="h-8 rounded-lg border border-border bg-surface px-3 text-sm font-medium transition hover:shadow-soft"
                >
                  Try again
                </button>
              }
            />
          ) : (
            <Outlet />
          )}
        </main>
      </div>
      <Inspector />
      <AppCommandPalette />
    </div>
    </InspectorProvider>
    </TourProvider>
    </CommandActionsProvider>
    </IngestEventsProvider>
    </OperationsProvider>
  )
}
