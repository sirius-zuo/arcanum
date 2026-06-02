import { BrowserRouter, Routes, Route, NavLink } from 'react-router-dom'
import SearchPage from './pages/SearchPage'
import DocsPage from './pages/DocsPage'
import CollectionsPage from './pages/CollectionsPage'
import { Search, FileText, Database } from 'lucide-react'

export default function App() {
  return (
    <BrowserRouter>
      <div className="flex h-screen overflow-hidden">
        {/* Sidebar */}
        <nav className="w-56 bg-[#13131f] border-r border-slate-800 flex flex-col p-4 gap-1 flex-shrink-0">
          <div className="text-sm font-mono text-blue-400 mb-6 px-2">
            devforge/docs
          </div>
          {[
            { to: '/', icon: Search, label: 'Search' },
            { to: '/docs', icon: FileText, label: 'Docs' },
            { to: '/collections', icon: Database, label: 'Collections' },
          ].map(({ to, icon: Icon, label }) => (
            <NavLink
              key={to}
              to={to}
              end={to === '/'}
              className={({ isActive }) =>
                `flex items-center gap-3 px-3 py-2 rounded-md text-sm transition-colors ${
                  isActive
                    ? 'bg-blue-600/20 text-blue-400'
                    : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800/50'
                }`
              }
            >
              <Icon size={16} />
              {label}
            </NavLink>
          ))}
        </nav>

        {/* Main content */}
        <main className="flex-1 overflow-auto p-6">
          <Routes>
            <Route path="/" element={<SearchPage />} />
            <Route path="/docs" element={<DocsPage />} />
            <Route path="/collections" element={<CollectionsPage />} />
          </Routes>
        </main>
      </div>
    </BrowserRouter>
  )
}
