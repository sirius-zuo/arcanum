import clsx from 'clsx'
import { PanelLeftClose, PanelLeftOpen } from 'lucide-react'
import { NavLink } from 'react-router-dom'
import { ROUTES } from '../routes'

interface RailProps {
  collapsed: boolean
  onToggle: () => void
}

export function Rail({ collapsed, onToggle }: RailProps) {
  return (
    <aside
      className={clsx(
        'sticky top-0 flex h-screen shrink-0 flex-col border-r border-border bg-surface/70 backdrop-blur transition-[width]',
        collapsed ? 'w-[68px]' : 'w-[232px]',
      )}
    >
      <div className={clsx('flex h-16 items-center gap-3 px-4', collapsed && 'justify-center px-0')}>
        <svg viewBox="0 0 32 32" className="h-8 w-8 shrink-0" aria-hidden="true">
          <rect width="32" height="32" rx="9" className="fill-accent" />
          <circle cx="16" cy="16" r="7" fill="none" strokeWidth="2" className="stroke-accent-fg" />
          <path d="M16 5v22M5 16h22" strokeWidth="1.5" opacity=".6" className="stroke-accent-fg" />
        </svg>
        {!collapsed && (
          <div className="leading-tight">
            <div className="text-[15px] font-semibold tracking-tight">Atlas</div>
            <div className="font-mono text-[10px] uppercase tracking-[0.14em] text-muted">Knowledge hub</div>
          </div>
        )}
      </div>

      <nav aria-label="Primary" className="flex-1 space-y-0.5 overflow-y-auto px-2 py-2">
        {ROUTES.map(({ path, label, icon: Icon }) => (
          <NavLink
            key={path}
            to={path}
            end={path === '/'}
            data-rail-item=""
            title={collapsed ? label : undefined}
            aria-label={collapsed ? label : undefined}
            className={({ isActive }) =>
              clsx(
                'group relative flex h-9 items-center gap-3 rounded-lg px-3 text-sm font-medium transition',
                collapsed && 'justify-center px-0',
                isActive ? 'bg-accent/10 text-accent' : 'text-muted hover:bg-surface-2 hover:text-text',
              )
            }
          >
            {({ isActive }) => (
              <>
                {isActive && <span className="absolute left-0 top-2 bottom-2 w-0.5 rounded-full bg-accent" aria-hidden="true" />}
                <Icon className="h-[18px] w-[18px] shrink-0" aria-hidden="true" />
                {!collapsed && <span>{label}</span>}
              </>
            )}
          </NavLink>
        ))}
      </nav>

      <div className="border-t border-border p-2">
        <button
          type="button"
          onClick={onToggle}
          aria-label={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
          className={clsx(
            'flex h-9 w-full items-center gap-3 rounded-lg px-3 text-sm text-muted transition hover:bg-surface-2 hover:text-text',
            collapsed && 'justify-center px-0',
          )}
        >
          {collapsed ? <PanelLeftOpen className="h-[18px] w-[18px]" aria-hidden="true" /> : <PanelLeftClose className="h-[18px] w-[18px]" aria-hidden="true" />}
          {!collapsed && <span>Collapse</span>}
        </button>
      </div>
    </aside>
  )
}
