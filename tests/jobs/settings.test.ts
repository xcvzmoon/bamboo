import * as v from 'valibot';
import { describe, expect, test } from 'vitest';
import { jobErrorSchema, jobSettingsSchema } from '../../app/utils/jobs.ts';

describe('job settings boundary', () => {
  test('parses edited JSON while preserving nested values and trimming the name', () => {
    expect(
      v.parse(jobSettingsSchema, {
        name: ' Wikipedia ',
        input: '{"topic":"Bamboo","options":[true,null,{"limit":3}]}',
        timeoutMs: 60_000,
      }),
    ).toEqual({
      name: 'Wikipedia',
      input: {
        topic: 'Bamboo',
        options: [true, null, { limit: 3 }],
      },
      timeoutMs: 60_000,
    });
  });

  test.each(['{', '{"limit":1e999}'])('rejects invalid JSON input %s', (input) => {
    expect(
      v.safeParse(jobSettingsSchema, {
        name: 'Job',
        input,
        timeoutMs: 1,
      }).success,
    ).toBe(false);
  });

  test.each([0, -1, 1.5, Number.MAX_SAFE_INTEGER + 1])(
    'rejects invalid timeout %s',
    (timeoutMs) => {
      expect(
        v.safeParse(jobSettingsSchema, {
          name: 'Job',
          input: '{}',
          timeoutMs,
        }).success,
      ).toBe(false);
    },
  );

  test('keeps stable Rust errors actionable', () => {
    expect(
      v.parse(jobErrorSchema, {
        code: 'STORAGE_IO',
        message: 'Entry file is missing',
      }),
    ).toBe('STORAGE_IO: Entry file is missing');
    expect(v.parse(jobErrorSchema, new Error('Invalid JSON'))).toBe('Invalid JSON');
    expect(v.parse(jobErrorSchema, null)).toBe('The operation failed. Please try again.');
  });
});
