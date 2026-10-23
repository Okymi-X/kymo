import { describe, expect, it } from 'vitest'
import { clampPage, pageCount, pageWindow, slicePage } from './paginate'

describe('paging maths', () => {
  it('counts at least one page', () => {
    expect(pageCount(0, 10)).toBe(1)
    expect(pageCount(10, 10)).toBe(1)
    expect(pageCount(11, 10)).toBe(2)
    expect(pageCount(100, 15)).toBe(7)
  })

  it('clamps and slices', () => {
    const items = Array.from({ length: 23 }, (_, i) => i)
    expect(clampPage(0, 23, 10)).toBe(1)
    expect(clampPage(9, 23, 10)).toBe(3)
    expect(slicePage(items, 1, 10)).toEqual(items.slice(0, 10))
    expect(slicePage(items, 3, 10)).toEqual([20, 21, 22])
    expect(slicePage(items, 99, 10)).toEqual([20, 21, 22])
    expect(slicePage([], 1, 10)).toEqual([])
  })
})

describe('pageWindow', () => {
  it('shows every page when there are few', () => {
    expect(pageWindow(2, 4)).toEqual([1, 2, 3, 4])
  })

  it('puts gaps between the ends and the current page', () => {
    expect(pageWindow(10, 20)).toEqual([1, 'gap', 9, 10, 11, 'gap', 20])
    expect(pageWindow(1, 20)).toEqual([1, 2, 'gap', 20])
    expect(pageWindow(20, 20)).toEqual([1, 'gap', 19, 20])
    expect(pageWindow(3, 20)).toEqual([1, 2, 3, 4, 'gap', 20])
  })
})
