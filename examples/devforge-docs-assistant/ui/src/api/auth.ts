// Single source of truth for the API key across all API modules.
export const apiKey: string =
  import.meta.env.VITE_API_KEY ??
  localStorage.getItem('arcanum_key') ??
  ''
