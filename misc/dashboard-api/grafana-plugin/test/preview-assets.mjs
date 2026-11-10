import {writeFile} from 'node:fs/promises';
await writeFile('test-build/index.html','<!doctype html><html><body><div id="root"></div><script src="preview.js"></script></body></html>');
