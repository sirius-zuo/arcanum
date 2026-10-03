import { EmptyState } from '../components/EmptyState'
import { PageHeader } from '../components/PageHeader'
import { ROUTES, routeMeta } from '../routes'

const meta = routeMeta('/context')
const step = String(ROUTES.indexOf(meta) + 1).padStart(2, '0')

export default function ContextPage() {
  return (
    <>
      <PageHeader eyebrow={`${step} / ${meta.label}`} title={meta.label} description={meta.blurb} />
      <EmptyState icon={meta.icon} title="Coming together" description="This screen is scaffolded and gets its content in a later task." />
    </>
  )
}
