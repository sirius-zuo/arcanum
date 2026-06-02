import { BrowserRouter, Routes, Route, NavLink } from 'react-router-dom'
import SearchClausesPage from './pages/SearchClausesPage'
import ContractLibraryPage from './pages/ContractLibraryPage'
import PartiesPage from './pages/PartiesPage'
import { Search, FileText, Users, Scale } from 'lucide-react'

export default function App() {
  return (
    <BrowserRouter>
      <div className="flex h-screen overflow-hidden">
        <nav className="w-60 bg-white border-r border-slate-200 flex flex-col p-4 gap-1 flex-shrink-0">
          <div className="flex items-center gap-2 text-slate-800 font-serif text-lg mb-1 px-2">
            <Scale size={20} className="text-slate-700" /> Vantage
          </div>
          <div className="text-xs text-slate-400 mb-6 px-2">Contract Intelligence</div>
          {[
            { to: '/', icon: Search, label: 'Search Clauses' },
            { to: '/library', icon: FileText, label: 'Contract Library' },
            { to: '/parties', icon: Users, label: 'Parties' },
          ].map(({ to, icon: Icon, label }) => (
            <NavLink
              key={to}
              to={to}
              end={to === '/'}
              className={({ isActive }) =>
                `flex items-center gap-3 px-3 py-2 rounded-lg text-sm transition-colors ${
                  isActive ? 'bg-slate-100 text-slate-900 font-medium' : 'text-slate-600 hover:bg-slate-100'
                }`
              }
            >
              <Icon size={16} /> {label}
            </NavLink>
          ))}
        </nav>
        <main className="flex-1 overflow-auto p-8">
          <Routes>
            <Route path="/" element={<SearchClausesPage />} />
            <Route path="/library" element={<ContractLibraryPage />} />
            <Route path="/parties" element={<PartiesPage />} />
          </Routes>
        </main>
      </div>
    </BrowserRouter>
  )
}
