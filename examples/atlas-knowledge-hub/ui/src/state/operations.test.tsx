import { beforeEach, describe, expect, it } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { OperationsProvider, useOperations } from './operations'

function Probe() {
  const { ops, track } = useOperations()
  return (
    <div>
      <button onClick={() => track([{ source_uri: 'a.md', operation_id: 'op-1' }])}>track</button>
      <output data-testid="count">{ops.length}</output>
    </div>
  )
}

function mount() {
  return render(
    <OperationsProvider>
      <Probe />
    </OperationsProvider>,
  )
}

describe('operations', () => {
  beforeEach(() => localStorage.clear())

  it('operations_persist_and_dedupe', async () => {
    const user = userEvent.setup()
    const first = mount()
    await user.click(screen.getByText('track'))
    await user.click(screen.getByText('track'))
    expect(screen.getByTestId('count')).toHaveTextContent('1')
    first.unmount()

    mount()
    expect(screen.getByTestId('count')).toHaveTextContent('1')
  })

  it('corrupted_storage_yields_an_empty_list', () => {
    localStorage.setItem('atlas.operations', '{not json')
    mount()
    expect(screen.getByTestId('count')).toHaveTextContent('0')
  })
})
