export const apiKey =
  import.meta.env.VITE_API_KEY ??
  localStorage.getItem('arcanum_key') ??
  ''
