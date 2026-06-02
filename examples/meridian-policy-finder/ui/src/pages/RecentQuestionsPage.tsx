import { useEffect, useState } from 'react'
import { Trash2, Hash, Sparkles } from 'lucide-react'

interface HistoryEntry {
  question: string
  strategy: string
  topTitle: string
  ts: number
}

function StrategyTag({ strategy }: { strategy: string }) {
  if (strategy === 'Bm25')
    return <span className="inline-flex items-center gap-1 text-xs text-orange-700"><Hash size={11} /> BM25</span>
  if (strategy === 'Vector')
    return <span className="inline-flex items-center gap-1 text-xs text-blue-700"><Sparkles size={11} /> Semantic</span>
  return <span className="text-xs text-slate-500">Combined</span>
}

export default function RecentQuestionsPage() {
  const [history, setHistory] = useState<HistoryEntry[]>([])

  useEffect(() => {
    setHistory(JSON.parse(localStorage.getItem('meridian_history') ?? '[]'))
  }, [])

  function clear() {
    localStorage.removeItem('meridian_history')
    setHistory([])
  }

  return (
    <div className="max-w-3xl">
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-semibold text-slate-900">Recent Questions</h1>
          <p className="text-slate-500 text-sm">Your search history (stored locally in this browser).</p>
        </div>
        {history.length > 0 && (
          <button onClick={clear} className="inline-flex items-center gap-2 text-sm text-slate-500 hover:text-red-600 transition-colors">
            <Trash2 size={14} /> Clear
          </button>
        )}
      </div>

      {history.length === 0 ? (
        <p className="text-slate-400 text-sm">No questions yet. Ask something on the Ask page.</p>
      ) : (
        <div className="bg-white border border-slate-200 rounded-xl overflow-hidden shadow-sm">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-slate-200 text-left text-slate-500">
                <th className="px-4 py-3 font-medium">Question</th>
                <th className="px-4 py-3 font-medium">Routed</th>
                <th className="px-4 py-3 font-medium">When</th>
              </tr>
            </thead>
            <tbody>
              {history.map((h, i) => (
                <tr key={i} className="border-b border-slate-100 last:border-0 hover:bg-slate-50">
                  <td className="px-4 py-3 text-slate-700">{h.question}</td>
                  <td className="px-4 py-3"><StrategyTag strategy={h.strategy} /></td>
                  <td className="px-4 py-3 text-slate-400 text-xs">{new Date(h.ts).toLocaleString()}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}
