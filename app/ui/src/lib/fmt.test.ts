import { expect, test } from 'vitest';
import { duration, gb, bar, tps } from './fmt';
import cases from '../fixtures/time_cases.json';
test('shared duration cases', () => {
  for (const [s, want] of cases.duration)
    expect(duration(Number(s))).toBe(want);
});
test('number formats', () => {
  expect(gb(22082528352)).toBe('22.1');
  expect(bar(0.5, 4)).toBe('▓▓░░');
  expect(bar(NaN, 2)).toBe('░░');
  expect(tps(503.9)).toBe('504');
  expect(tps(45.1)).toBe('45.1');
  expect(tps()).toBe('—');
});
