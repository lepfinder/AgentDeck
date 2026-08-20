import { writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { getApiDocsMarkdown } from '../src/utils/apiDocsMarkdown.ts';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const out = join(root, 'src-tauri/src/api_docs.generated.md');

writeFileSync(out, getApiDocsMarkdown(), 'utf8');
console.log(`Generated ${out}`);
