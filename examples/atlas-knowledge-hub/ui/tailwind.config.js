const token = (name) => `rgb(var(${name}) / <alpha-value>)`

/** @type {import('tailwindcss').Config} */
export default {
  darkMode: 'class',
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      colors: {
        bg: token('--bg'),
        surface: token('--surface'),
        'surface-2': token('--surface-2'),
        border: token('--border'),
        text: token('--text'),
        muted: token('--muted'),
        accent: token('--accent'),
        'accent-fg': token('--accent-fg'),
        v: {
          supported: token('--v-supported'),
          partial: token('--v-partial'),
          unsupported: token('--v-unsupported'),
          miscited: token('--v-miscited'),
          uncited: token('--v-uncited'),
          noclaim: token('--v-noclaim'),
        },
        vink: {
          supported: token('--vink-supported'),
          partial: token('--vink-partial'),
          unsupported: token('--vink-unsupported'),
          miscited: token('--vink-miscited'),
          uncited: token('--vink-uncited'),
          noclaim: token('--vink-noclaim'),
        },
      },
      fontFamily: {
        sans: ['Inter', 'ui-sans-serif', 'system-ui', 'sans-serif'],
        mono: ['"JetBrains Mono"', 'ui-monospace', 'SFMono-Regular', 'monospace'],
      },
      borderRadius: { card: '12px' },
      boxShadow: {
        soft: 'var(--shadow-soft)',
        lift: 'var(--shadow-lift)',
      },
      transitionDuration: { DEFAULT: '150ms' },
      keyframes: {
        caret: { to: { visibility: 'hidden' } },
        'chip-in': { from: { opacity: '0', transform: 'translateY(2px) scale(0.96)' }, to: { opacity: '1', transform: 'none' } },
        shimmer: { '100%': { transform: 'translateX(100%)' } },
        rise: {
          from: { opacity: '0', transform: 'translateY(6px)' },
          to: { opacity: '1', transform: 'translateY(0)' },
        },
      },
      animation: { 'chip-in': 'chip-in 180ms ease-out both', rise: 'rise 260ms cubic-bezier(0.2, 0.7, 0.2, 1) both' },
    },
  },
  plugins: [],
}
