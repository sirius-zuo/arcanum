import { BrowserRouter, Routes, Route, NavLink } from 'react-router-dom'
import SearchPage from './pages/SearchPage'
import CatalogPage from './pages/CatalogPage'
import SupportLookupPage from './pages/SupportLookupPage'
import { Search, Package, Headphones, Mountain } from 'lucide-react'

export default function App() {
  return (
    <BrowserRouter>
      <div className="min-h-screen flex flex-col">
        <header className="border-b border-stone-200 bg-white sticky top-0 z-10">
          <div className="max-w-6xl mx-auto px-6 h-16 flex items-center gap-8">
            <div className="flex items-center gap-2 text-green-800 font-semibold text-lg">
              <Mountain size={22} /> Canopy
            </div>
            <nav className="flex items-center gap-1">
              {[
                { to: '/', icon: Search, label: 'Search' },
                { to: '/catalog', icon: Package, label: 'Catalog' },
                { to: '/support', icon: Headphones, label: 'Support Lookup' },
              ].map(({ to, icon: Icon, label }) => (
                <NavLink
                  key={to}
                  to={to}
                  end={to === '/'}
                  className={({ isActive }) =>
                    `flex items-center gap-2 px-3 py-2 rounded-lg text-sm transition-colors ${
                      isActive ? 'bg-green-50 text-green-800 font-medium' : 'text-stone-600 hover:bg-stone-100'
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
            <Route path="/" element={<SearchPage />} />
            <Route path="/catalog" element={<CatalogPage />} />
            <Route path="/support" element={<SupportLookupPage />} />
          </Routes>
        </main>
      </div>
    </BrowserRouter>
  )
}
