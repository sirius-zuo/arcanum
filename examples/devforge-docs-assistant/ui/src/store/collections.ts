// localStorage-based collection tracking removed — server is source of truth.
try {
  localStorage.removeItem('arcanum_collections')
  localStorage.removeItem('arcanum_known_collections')
} catch {}
