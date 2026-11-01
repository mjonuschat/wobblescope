import { fireEvent, screen, waitFor } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import type { Reply } from '@/test/http'
import { stubApiWithStatus as stubApi } from '@/test/status'
import { renderRoute } from '@/test/render'
import { summary } from '@/test/summaries'

function api(onImport: Reply | ((search: URLSearchParams) => Reply) = { status: 201, body: summary('csv-20261031T120000Z') }) {
  return stubApi(({ url, method }) => {
    if (method === 'POST') return typeof onImport === 'function' ? onImport(url.searchParams) : onImport
    if (url.pathname.endsWith('/storage')) return { status: 503, body: { error: 'storage_unavailable', message: 'x' } }
    if (url.pathname.endsWith('/sessions')) return { body: { sessions: [summary('s1')] } }
    return { status: 404, body: { error: 'not_found', message: 'no' } }
  })
}

async function openDialog() {
  const { router } = renderRoute('/sessions')
  fireEvent.click(await screen.findByRole('button', { name: 'Import CSV' }))
  await screen.findByRole('dialog')
  return router
}

function fill({ file, chip }: { file?: File; chip?: string }) {
  if (file) fireEvent.change(screen.getByLabelText('CSV file'), { target: { files: [file] } })
  if (chip !== undefined) fireEvent.change(screen.getByLabelText('Chip name'), { target: { value: chip } })
}

const submit = () => fireEvent.click(screen.getByRole('button', { name: 'Import' }))
const csv = (name = 'raw_data_x.csv') => new File(['1,2,3,4'], name, { type: 'text/csv' })
const posts = (calls: { method: string }[]) => calls.filter((c) => c.method === 'POST')

describe('the import dialog', () => {
  it('sends nothing without a chip name and marks the field', async () => {
    const calls = api()
    await openDialog()
    fill({ file: csv() })
    submit()
    expect(await screen.findByText('Enter the chip name.')).toBeTruthy()
    expect(screen.getByLabelText('Chip name').getAttribute('aria-invalid')).toBe('true')
    expect(posts(calls)).toHaveLength(0)
  })

  it('refuses a file one byte over 64 MiB without sending it', async () => {
    const calls = api()
    await openDialog()
    const big = csv()
    Object.defineProperty(big, 'size', { value: 64 * 1024 * 1024 + 1 })
    fill({ file: big, chip: 'adxl345' })
    submit()
    expect(await screen.findByText(/The limit is 64 MiB/)).toBeTruthy()
    expect(posts(calls)).toHaveLength(0)
  })

  it('sends the file name and no axis when the axis is left empty', async () => {
    const calls = api()
    await openDialog()
    fill({ file: csv('raw_data_y_adxl345_100.000_100.000_20.000_test.csv'), chip: 'adxl345' })
    submit()
    await waitFor(() => expect(posts(calls)).toHaveLength(1))
    const { url } = posts(calls)[0] as unknown as { url: URL }
    expect(url.searchParams.get('file_name')).toBe('raw_data_y_adxl345_100.000_100.000_20.000_test.csv')
    expect(url.searchParams.get('chip')).toBe('adxl345')
    expect(url.searchParams.has('axis')).toBe(false)
  })

  it('covers AE1: a 400 shows its message and keeps the form values', async () => {
    api({ status: 400, body: { error: 'invalid_request', message: 'cannot tell the axis from capture.csv' } })
    await openDialog()
    fill({ file: csv('capture.csv'), chip: 'adxl345' })
    submit()
    expect(await screen.findByText('cannot tell the axis from capture.csv')).toBeTruthy()
    expect((screen.getByLabelText('Chip name') as HTMLInputElement).value).toBe('adxl345')
    expect(screen.getByRole('dialog')).toBeTruthy()
  })

  it('shows a cap refusal with the needed and capped bytes', async () => {
    api({
      status: 507,
      body: { error: 'cap_exceeded', message: 'not enough room', need: 3 * 1024 ** 2, cap: 2 * 1024 ** 3, pinned_bytes: 0, evictable_bytes: 0 },
    })
    await openDialog()
    fill({ file: csv(), chip: 'adxl345' })
    submit()
    const alert = await screen.findByText(/not enough room/)
    expect(alert.textContent).toContain('needs 3 MiB')
    expect(alert.textContent).toContain('cap is 2 GiB')
  })

  it('says another import is running on a 429', async () => {
    api({ status: 429, body: { error: 'busy', message: 'an import is already running' } })
    await openDialog()
    fill({ file: csv(), chip: 'adxl345' })
    submit()
    expect(await screen.findByText(/Another import is running/)).toBeTruthy()
  })

  it('opens the new session after a 201', async () => {
    api()
    const router = await openDialog()
    fill({ file: csv(), chip: 'adxl345' })
    submit()
    await waitFor(() =>
      expect(router.state.location.pathname).toBe('/sessions/csv-20261031T120000Z'),
    )
  })
})

describe('reopening the import dialog', () => {
  it('starts empty, so a file chosen before closing is never uploaded unseen', async () => {
    const calls = api()
    await openDialog()
    fill({ file: csv('stale.csv'), chip: 'adxl345' })
    fireEvent.click(screen.getByRole('button', { name: 'Close' }))
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
    fireEvent.click(screen.getByRole('button', { name: 'Import CSV' }))
    await screen.findByRole('dialog')
    expect((screen.getByLabelText('Chip name') as HTMLInputElement).value).toBe('')
    fill({ chip: 'adxl345' })
    submit()
    expect(await screen.findByText('Choose a CSV file.')).toBeTruthy()
    expect(posts(calls)).toHaveLength(0)
  })
})
