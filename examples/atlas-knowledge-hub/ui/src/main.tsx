import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import App from './App'
import { BootstrapProvider } from './state/bootstrap'
import { ThemeProvider } from './state/theme'
import './index.css'

const queryClient = new QueryClient({
  defaultOptions: { queries: { refetchOnWindowFocus: false, staleTime: 5_000 } },
})

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <ThemeProvider>
      <BootstrapProvider>
        <QueryClientProvider client={queryClient}>
          <App />
        </QueryClientProvider>
      </BootstrapProvider>
    </ThemeProvider>
  </StrictMode>,
)
