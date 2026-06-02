import { BrowserRouter, Routes, Route, NavLink } from 'react-router-dom'
import SearchPage from './pages/SearchPage'
import LibraryPage from './pages/LibraryPage'
import DiscoverPage from './pages/DiscoverPage'
import { Search, BookMarked, Compass, Library } from 'lucide-react'

export default function App() {
  return (
    <BrowserRouter>
      <div className="min-h-screen flex flex-col">
        <header className="border-b border-stone-200 bg-[#fafaf7]/90 backdrop-blur sticky top-0 z-10">
          <div className="max-w-6xl mx-auto px-6 h-16 flex items-center gap-8">
            <div className="flex items-center gap-2 text-amber-700 font-serif text-xl">
              <Library size={22} /> Folio
            </div>
            <nav className="flex items-center gap-1">
              {[
                { to: '/', icon: BookMarked, label: 'My Library' },
                { to: '/search', icon: Search, label: 'Search' },
                { to: '/discover', icon: Compass, label: 'Discover' },
              ].map(({ to, icon: Icon, label }) => (
                <NavLink
                  key={to}
                  to={to}
                  end={to === '/'}
                  className={({ isActive }) =>
                    `flex items-center gap-2 px-3 py-2 rounded-lg text-sm transition-colors ${
                      isActive ? 'bg-amber-50 text-amber-800 font-medium' : 'text-stone-600 hover:bg-stone-100'
                    }`
                  }
                >
                  <Icon size={15} /> {label}
                </NavLink>
              ))}
            </nav>
          </div>
        </header>
        <main className="flex-1 max-w-6xl mx-auto w-full px-6 py-8">
          <Routes>
            <Route path="/" element={<LibraryPage />} />
            <Route path="/search" element={<SearchPage />} />
            <Route path="/discover" element={<DiscoverPage />} />
          </Routes>
        </main>
      </div>
    </BrowserRouter>
  )
}