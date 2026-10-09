import { mkdir, copyFile } from 'node:fs/promises';
await mkdir('dist/img', { recursive: true });
await copyFile('src/plugin.json', 'dist/plugin.json');
await copyFile('src/img/logo.svg', 'dist/img/logo.svg');
await mkdir('dist-workday/img', { recursive: true });
await copyFile('src/workday-plugin.json', 'dist-workday/plugin.json');
await copyFile('src/img/logo.svg', 'dist-workday/img/logo.svg');
