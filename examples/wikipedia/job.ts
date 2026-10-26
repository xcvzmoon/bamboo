import type { JobContext, JobResult } from '../../runner/job.ts';
import * as v from 'valibot';

const inputSchema = v.object({
  topic: v.pipe(v.string(), v.trim(), v.nonEmpty(), v.maxLength(200)),
});

export default async function wikipedia({ context, input }: JobContext): Promise<JobResult> {
  const { topic } = v.parse(inputSchema, input);
  const page = await context.newPage();
  const url = `https://en.wikipedia.org/wiki/${encodeURIComponent(topic.replaceAll(' ', '_'))}`;
  const response = await page.goto(url, {
    timeout: 30_000,
    waitUntil: 'domcontentloaded',
  });

  if (!response?.ok()) {
    throw new Error(`WIKIPEDIA_HTTP: Article request failed with status ${response?.status()}`);
  }

  const title = await page.locator('h1').innerText({ timeout: 5000 });
  const paragraphs = await page.locator('.mw-parser-output p').allTextContents();
  const records: JobResult = [];

  for (const text of paragraphs) {
    const paragraph = text.trim();
    if (paragraph) {
      records.push({
        title,
        url: page.url(),
        text: paragraph,
      });
    }
  }

  if (records.length === 0) {
    throw new Error('WIKIPEDIA_EMPTY: Article contained no paragraphs');
  }

  return records;
}
