import {describe,it,expect} from 'vitest'
import {providerCost} from '../src/shared/lib/format'
describe('provider cost knowledge',()=>{
 it('keeps absent pricing distinct from an explicitly free operation',()=>{
  expect(providerCost(null)).toBe('неизвестна')
  expect(providerCost(undefined)).toBe('неизвестна')
  expect(providerCost(NaN)).toBe('неизвестна')
  expect(providerCost(0)).toBe('0')
  expect(providerCost(0.000125)).toBe('0,000125')
 })
})
