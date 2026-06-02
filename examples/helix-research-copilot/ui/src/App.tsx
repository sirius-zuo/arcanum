import { BrowserRouter, Routes, Route, NavLink } from 'react-router-dom'
import CopilotPage from './pages/CopilotPage'
import CorpusPage from './pages/CorpusPage'
import KnowledgeGraphPage from './pages/KnowledgeGraphPage'
import { Sparkles, FlaskConical, Network } from 'lucide-react'

export default function App() {
  return (
    <BrowserRouter>
      <div className="flex h-screen overflow-hidden">
        <nav className="w-60 bg-[#0f0f16] border-r border-slate-800 flex flex-col p-4 gap-1 flex-shrink-0">
          <div className="text-base font-semibold text-teal-400 mb-1 px-2">Helix</div>
          <div className="text-xs text-slate-500 mb-6 px-2 font-mono">research copilot</div>
          {[
            { to: '/', icon: Sparkles, label: 'Copilot' },
            { to: '/corpus', icon: FlaskConical, label: 'Research Corpus' },
            { to: '/graph', icon: Network, label: 'Knowledge Graph' },
          ].map(({ to, icon: Icon, label }) => (
            <NavLink
              key={to}
              to={to}
              end={to === '/'}
              className={({ isActive }) =>
                `flex items-center gap-3 px-3 py-2 rounded-lg text-sm transition-colors ${
                  isActive ? 'bg-teal-500/15 text-teal-300' : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800/50'
                }`
              }
            >
              <Icon size={16} /> {label}
            </NavLink>
          ))}
        </nav>
        <main className="flex-1 overflow-auto p-8">
          <Routes>
            <Route path="/" element={<CopilotPage />} />
            <Route path="/corpus" element={<CorpusPage />} />
            <Route path="/graph" element={<KnowledgeGraphPage />} />
          </Routes>
        </main>
      </div>
    </BrowserRouter>
  )
}
