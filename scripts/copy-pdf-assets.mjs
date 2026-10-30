import { cp, mkdir, copyFile } from 'node:fs/promises';
await mkdir('public/pdf', { recursive: true });
for (const folder of ['standard_fonts', 'cmaps'])
  await cp(`node_modules/pdfjs-dist/${folder}`, `public/pdf/${folder}`, { recursive: true });
await copyFile('node_modules/pdfjs-dist/LICENSE', 'public/pdf/LICENSE');
