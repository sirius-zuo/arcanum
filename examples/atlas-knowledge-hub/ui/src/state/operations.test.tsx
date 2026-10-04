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

  it('replay_marks_the_existing_entry_and_persists', async () => {
    const user = userEvent.setup()
    function Replayer() {
      const { ops, track } = useOperations()
      return (
        <div>
          <button onClick={() => track([{ source_uri: 'x', operation_id: 'a' }])}>first</button>
          <button onClick={() => track([{ source_uri: 'x', operation_id: 'a', replay: true }])}>replay</button>
          <output data-testid="state">{JSON.stringify(ops.map((o) => [o.operation_id, o.replay === true, o.addedAt]))}</output>
        </div>
      )
    }
    const view = render(
      <OperationsProvider>
        <Replayer />
      </OperationsProvider>,
    )
    await user.click(screen.getByText('first'))
    const before = JSON.parse(screen.getByTestId('state').textContent!) as [string, boolean, number][]
    await user.click(screen.getByText('replay'))
    await user.click(screen.getByText('first'))
    const after = JSON.parse(screen.getByTestId('state').textContent!) as [string, boolean, number][]
    expect(after).toHaveLength(1)
    expect(after[0][1]).toBe(true)
    expect(after[0][2]).toBe(before[0][2])
    view.unmount()
    render(
      <OperationsProvider>
        <Replayer />
      </OperationsProvider>,
    )
    expect(JSON.parse(screen.getByTestId('state').textContent!)).toEqual([['a', true, before[0][2]]])
  })
})
