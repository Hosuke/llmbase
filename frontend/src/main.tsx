import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { ThemeProvider } from './lib/theme'
import { LangProvider } from './lib/lang'
import './index.css'
import App from './App'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <ThemeProvider>
      <LangProvider>
        <App />
      </LangProvider>
    </ThemeProvider>
  </StrictMode>,
)
