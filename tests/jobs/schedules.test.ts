import * as v from 'valibot';
import { describe, expect, test } from 'vitest';
import { parseScheduleForm, scheduleSchema } from '../../app/utils/schedules.ts';

describe('schedule form boundary', () => {
  test('normalizes daily times and preserves interval units', () => {
    expect(parseScheduleForm('daily', 0, 'minutes', '09:05')).toEqual({
      kind: 'daily',
      hour: 9,
      minute: 5,
    });
    expect(parseScheduleForm('interval', 2, 'hours', '')).toEqual({
      kind: 'interval',
      every: 2,
      unit: 'hours',
    });
    expect(parseScheduleForm('disabled', NaN, 'minutes', '')).toEqual({ kind: 'disabled' });
  });

  test.each(['', '9:00', '24:00', '12:60', '09:00:30'])('rejects invalid daily time %s', (time) => {
    expect(() => parseScheduleForm('daily', 1, 'minutes', time)).toThrow();
  });

  test.each([0, -1, 1.5, NaN, Infinity, 4_294_967_296])('rejects invalid interval %s', (every) => {
    expect(() => parseScheduleForm('interval', every, 'minutes', '09:00')).toThrow();
  });

  test('rejects malformed schedule responses at the command boundary', () => {
    expect(
      v.safeParse(scheduleSchema, {
        kind: 'daily',
        hour: 25,
        minute: 0,
      }).success,
    ).toBe(false);
    expect(
      v.safeParse(scheduleSchema, {
        kind: 'interval',
        every: 1,
        unit: 'days',
      }).success,
    ).toBe(false);
  });
});
