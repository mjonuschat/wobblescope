import { describe, expect, it } from 'vitest'

import { setBase, stubFetch } from '@/test/http'

import {
  deleteSession,
  fetchSample,
  importCsv,
  listSessions,
  pinSession,
} from './endpoints'

describe('endpoints', () => {
  it('lists with filters as query parameters', async () => {
    setBase('http://printer.local/wobblescope/')
    const http = stubFetch({ body: { sessions: [] } })
    await listSessions({ chip: 'adxl345 nh36', pinned: true })
    expect(http.url()).toBe(
      'http://printer.local/wobblescope/api/sessions?chip=adxl345+nh36&pinned=true',
    )
  })

  it('pins with a JSON body', async () => {
    const http = stubFetch({ body: { id: 's1', pinned: true } })
    await pinSession('s1', true)
    expect(http.init().method).toBe('PATCH')
    expect(http.init().body).toBe('{"pinned":true}')
  })

  it('deletes with DELETE', async () => {
    const http = stubFetch({ status: 204 })
    await deleteSession('s1')
    expect(http.init().method).toBe('DELETE')
    expect(http.url()).toMatch(/\/api\/sessions\/s1$/)
  })

  it('downloads the exact bytes of one recording', async () => {
    const bytes = new Uint8Array([1, 2, 3, 250]).buffer
    const http = stubFetch({ bytes })
    const got = await fetchSample('s1', 1, 0)
    expect(http.url()).toMatch(/\/api\/sessions\/s1\/samples\/1\/0$/)
    expect([...new Uint8Array(got)]).toEqual([1, 2, 3, 250])
  })

  it('imports the file itself with chip, axis, point and file name', async () => {
    const http = stubFetch({ status: 201, body: { id: 'csv-1' } })
    const file = new File(['a,b'], 'run.csv', { type: 'text/csv' })
    await importCsv({ file, chip: 'adxl345', axis: 'X', point: '1,2,3' })
    const url = new URL(http.url())
    expect(http.init().method).toBe('POST')
    expect(http.init().body).toBe(file)
    expect(Object.fromEntries(url.searchParams)).toEqual({
      chip: 'adxl345',
      axis: 'x',
      point: '1,2,3',
      file_name: 'run.csv',
    })
  })

  it('leaves an empty axis and point out of the query', async () => {
    const http = stubFetch({ status: 201, body: { id: 'csv-1' } })
    await importCsv({ file: new File(['x'], 'a.csv'), chip: 'adxl345', axis: '', point: '' })
    const url = new URL(http.url())
    expect(url.searchParams.has('axis')).toBe(false)
    expect(url.searchParams.has('point')).toBe(false)
  })
})
