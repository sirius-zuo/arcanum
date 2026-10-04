import { createBrowserRouter, RouterProvider } from 'react-router-dom'
import { AppShell } from './components/AppShell'
import OverviewPage from './pages/OverviewPage'
import LibraryPage from './pages/LibraryPage'
import SearchPage from './pages/SearchPage'
import ContextPage from './pages/ContextPage'
import AskPage from './pages/AskPage'
import VerifyPage from './pages/VerifyPage'
import EvidencePage from './pages/EvidencePage'
import GraphPage from './pages/GraphPage'
import LabPage from './pages/LabPage'
import AdminPage from './pages/AdminPage'
import ConnectPage from './pages/ConnectPage'

export const router = createBrowserRouter([
  {
    path: '/',
    element: <AppShell />,
    children: [
      { index: true, element: <OverviewPage /> },
      { path: 'library', element: <LibraryPage /> },
      { path: 'search', element: <SearchPage /> },
      { path: 'context', element: <ContextPage /> },
      { path: 'ask', element: <AskPage /> },
      { path: 'verify', element: <VerifyPage /> },
      { path: 'evidence', element: <EvidencePage /> },
      { path: 'graph', element: <GraphPage /> },
      { path: 'lab', element: <LabPage /> },
      { path: 'admin', element: <AdminPage /> },
      { path: 'connect', element: <ConnectPage /> },
    ],
  },
])

export default function App() {
  return <RouterProvider router={router} />
}
