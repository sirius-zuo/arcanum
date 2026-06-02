import { BrowserRouter, Routes, Route, NavLink } from 'react-router-dom'
import AskPage from './pages/AskPage'
import PolicyLibraryPage from './pages/PolicyLibraryPage'
import RecentQuestionsPage from './pages/RecentQuestionsPage'
import { MessageCircleQuestion, Library, History } from 'lucide-react'

export default function App() {
  return (
    <BrowserRouter>
      <div className="flex h-screen overflow-hidden">
        <nav className="w-60 bg-white border-r border-slate-200 flex flex-col p-4 gap-1 flex-shrink-0">
          <div className="text-base font-semibold text-slate-800 mb-1 px-2">Meridian</div>
          <div className="text-xs text-slate-400 mb-6 px-2">Policy Finder</div>
          {[
            { to: '/', icon: MessageCircleQuestion, label: 'Ask a Question' },
            { to: '/library', icon: Library, label: 'Policy Library' },
            { to: '/history', icon: History, label: 'Recent Questions' },
          ].map(({ to, icon: Icon, label }) => (
            <NavLink
              key={to}
              to={to}
              end={to === '/'}
              className={({ isActive }) =>
                `flex items-center gap-3 px-3 py-2 rounded-lg text-sm transition-colors ${
                  isActive
                    ? 'bg-blue-50 text-blue-700 font-medium'
                    : 'text-slate-600 hover:text-slate-900 hover:bg-slate-100'
                }`
              }
            >
              <Icon size={16} />
              {label}
            </NavLink>
          ))}
        </nav>
        <main className="flex-1 overflow-auto p-8">
          <Routes>
            <Route path="/" element={<AskPage />} />
            <Route path="/library" element={<PolicyLibraryPage />} />
            <Route path="/history" element={<RecentQuestionsPage />} />
          </Routes>
        </main>
      </div>
    </BrowserRouter>
  )
}
