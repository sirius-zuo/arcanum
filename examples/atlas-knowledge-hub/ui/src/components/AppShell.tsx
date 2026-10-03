import { useState } from 'react'
import { Outlet } from 'react-router-dom'
import { Rail } from './Rail'
import { TopBar } from './TopBar'
import { ErrorState } from './ErrorState'
import { useBootstrap } from '../state/bootstrap'
import { safeGet, safeSet } from '../lib/storage'

const KEY = 'atlas.rail.collapsed'

function initialCollapsed(): boolean {
  const stored = safeGet(KEY)
  if (stored === '1') return true
  if (stored === '0') return false
  return typeof window !== 'undefined' && window.innerWidth < 1280
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
    <div className="flex min-h-screen">
      <Rail collapsed={collapsed} onToggle={toggle} />
      <div className="min-w-0 flex-1">
        <TopBar />
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
    </div>
  )
}
